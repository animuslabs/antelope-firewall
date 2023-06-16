use itertools::Itertools;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use reqwest::Url;
use tokio::sync::Mutex;
use thiserror::Error;
use std::collections::{HashSet, HashMap};
use std::net::{SocketAddr, IpAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::net::TcpListener;

use crate::ratelimiter::IncrementMode;
use crate::{RequestInfo, MatchingFn};
use crate::matching_engine::MatchingEngine;
use crate::util::{get_blocked_response, get_ratelimit_response, get_error_response, full};
use crate::{filter::Filter, ratelimiter::RateLimiter};

use hyper::server::conn::http1;
use hyper::{Request, Response};
use hyper::service::service_fn;
use hyper::body::Bytes;

use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use itertools::FoldWhile::{Continue, Done};

pub enum RoutingModeState {
    RoundRobin(HashMap<String, AtomicU64>),
    LeastConnected(HashMap<String, AtomicU64>),
    Random
}

impl RoutingModeState {
    pub fn base_round_robin() -> Self {
        RoutingModeState::RoundRobin(HashMap::new())
    }
    pub fn base_least_connected() -> Self {
        RoutingModeState::LeastConnected(HashMap::new())
    }
    pub fn base_random() -> Self {
        RoutingModeState::Random
    }
}

pub struct AntelopeFirewall {
    filters: Vec<Filter>,
    ratelimiters: Vec<Mutex<RateLimiter<String>>>,
    matching_engine: MatchingEngine,
    routing_mode: RoutingModeState
}

#[derive(Error, Debug, Clone)]
pub enum AntelopeFirewallError {
    #[error("Failed to start a server on port: `{1}`, received error: `{0}`")]
    StartingServerFailed(String, u16),
    #[error("Failed to accept a new TCP connection, received error: `{0}`")]
    AcceptTCPConnectionFailed(String),
    #[error("Failed to parse the request body, received error: `{0}`")]
    ParseBodyFailed(String),
}

use AntelopeFirewallError::*;

impl AntelopeFirewall {
    pub fn new(routing_mode: RoutingModeState) -> Self {
        AntelopeFirewall {
            filters: Vec::new(),
            ratelimiters: Vec::new(),
            matching_engine: MatchingEngine::new(),
            routing_mode
        }
    }
    pub fn add_filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }
    pub fn add_ratelimiter(mut self, ratelimiter: RateLimiter<String>) -> Self {
        self.ratelimiters.push(Mutex::new(ratelimiter));
        self
    }
    pub fn add_rule(mut self, rule: Box<MatchingFn>) -> Self {
        self.matching_engine.add_rule(rule);
        self
    }

    pub fn build(self) -> Arc<Self> {
        Arc::new(self)
    }
    pub async fn run(self: Arc<Self>) -> Result<(), AntelopeFirewallError> {
        let port = 3000;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr).await
            .map_err(|e| AntelopeFirewallError::StartingServerFailed(e.to_string(), port))?;

        loop {
            let (stream, _) = listener.accept().await
                .map_err(|e| AntelopeFirewallError::AcceptTCPConnectionFailed(e.to_string()))?;

            let new_self = Arc::clone(&self);
            tokio::task::spawn(async move {
                let address = stream.peer_addr()
                    .map(|addr| addr.ip())
                    .unwrap_or(IpAddr::from([127, 0, 0, 1]));

                if let Err(err) = http1::Builder::new()
                    .serve_connection(stream, service_fn(|r| new_self.handle_request(r, address)))
                    .await
                {
                    println!("Error serving connection: {:?}", err);
                }
            });
        }
    }

    async fn handle_request(&self, req: Request<hyper::body::Incoming>, ip: IpAddr) -> Result<Response<BoxBody<Bytes, hyper::Error>>, AntelopeFirewallError> {
        // Parse thr request, try to put body into JSON
        let (parts, body) = req.into_parts();
        let request_info = Arc::new(RequestInfo::new(parts.headers.clone(), parts.uri, ip));
        let body_bytes = body.collect().await
            .map_err(|e| ParseBodyFailed(e.to_string()))?
            .to_bytes();
        let body_json = Arc::new(serde_json::from_slice::<serde_json::Value>(&body_bytes)
            .map_err(|e| ParseBodyFailed(e.to_string()))?);

        // Check if the request should be filtered out
        for filter in &self.filters {
            if !filter.should_request_pass(Arc::clone(&request_info), Arc::clone(&body_json)).await {
                return Ok(get_blocked_response());
            }
        }

        // Check if the request should be rate limited
        for ratelimiter in &self.ratelimiters {
            let mut ratelimiter = ratelimiter.lock().await;
            if !ratelimiter.should_request_pass(Arc::clone(&request_info), Arc::clone(&body_json)).await {
                return Ok(get_ratelimit_response(ratelimiter.get_window_duration()));
            }
        }
        
        // Find end nodes that can accept the request with the matching engine
        let urls = self.matching_engine.find_matching_urls(Arc::clone(&request_info), Arc::clone(&body_json)).await;
        if urls.len() == 0 {
            return Ok(get_error_response(full("Failed to find a route for your request.")));
        }

        let url = match self.routing_mode {
            RoutingModeState::LeastConnected(ref counts) => {
                urls.into_iter()
                    .map(|(url, weight)| (
                        url.clone(),
                        counts.get(&url.host().unwrap().to_string())
                            .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
                            .unwrap_or(1) as f32 / weight as f32
                    ))
                    .min_by(|(_, w1), (_, w2)| w1.partial_cmp(w2).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("There were no possible urls even though it was checked earlier")
                    .0
            },
            RoutingModeState::RoundRobin(ref counts) => {
                let count = counts.get(request_info.uri.path())
                    .map(|a| a.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
                    .unwrap_or(0);

                let modulated = count % urls.iter().map(|(_, weight)| weight).sum::<u64>();
                urls.iter().fold_while((modulated, Url::parse("127.0.0.1").unwrap()), |(weights_left, s), (url, weight)| {
                    if weights_left < *weight {
                        Done((0, url.clone()))
                    } else {
                        Continue((weights_left.checked_sub(*weight).unwrap_or(0), s))
                    }
                }).into_inner().1
            },
            RoutingModeState::Random => {
                let (urls, weights): (Vec<Url>, Vec<u64>) = urls.into_iter().unzip();
                let dist = WeightedIndex::new(weights).unwrap();
                urls[dist.sample(&mut rand::thread_rng())].clone()
            }
        };
        
        // Send the request
        let mut headers = parts.headers;
        headers.insert("X-Forwarded-For", ip.to_string().parse().unwrap());

        let client = reqwest::Client::new();
        let node_res = client.post(url)
            .headers(headers)
            .body(body_bytes)
            .send()
            .await.unwrap();
        
        // Update any ratelimiters that need to be notified on failure
        //if todo!() {
            //urls;
        //};

        
        // Respond to the client
        let mut client_res = Response::builder()
            .status(node_res.status());

        client_res.headers_mut().map(|h| h.clone_from(node_res.headers()));
        let final_response = client_res.body(full(node_res.bytes().await.unwrap())).unwrap();
        Ok(final_response)
    }
}

