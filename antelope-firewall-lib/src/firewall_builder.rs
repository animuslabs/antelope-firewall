use itertools::Itertools;
use prometheus_exporter::prometheus::core::{AtomicF64, GenericCounter};
use prometheus_exporter::prometheus::register_counter;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use reqwest::Url;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;

use crate::de::Transaction;
use crate::matching_engine::MatchingEngine;
use crate::prometheus::{REQUESTS_RECEIVED, REQUESTS_FAILED_TO_ROUTE, SUCCESS_NODE_RESPONSES, CLIENT_ERROR_NODE_RESPONSES, SERVER_ERROR_NODE_RESPONSES};
use crate::util::{full, get_blocked_response, get_error_response, get_ratelimit_response, get_options_response};
use crate::{filter::Filter, ratelimiter::RateLimiter};
use crate::{MatchingFn, RequestInfo};

use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};

use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use itertools::FoldWhile::{Continue, Done};

lazy_static::lazy_static! {
    static ref MATCHING_COUNTERS: RwLock<HashMap<String, (
        GenericCounter<AtomicF64>,
        GenericCounter<AtomicF64>,
        GenericCounter<AtomicF64>,
        GenericCounter<AtomicF64>
    )>> = RwLock::new(HashMap::new());
}

pub enum RoutingModeState {
    RoundRobin(HashMap<String, AtomicU64>),
    LeastConnected(HashMap<String, AtomicU64>),
    Random,
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
    ratelimiters: Vec<RateLimiter>,
    matching_engine: MatchingEngine,
    routing_mode: RoutingModeState,
    socket_addr: SocketAddr
}

#[derive(Error, Debug, Clone)]
pub enum AntelopeFirewallError {
    #[error("Failed to start a server on socket: `{1}`, received error: `{0}`")]
    StartingServerFailed(String, SocketAddr),
    #[error("Failed to accept a new TCP connection, received error: `{0}`")]
    AcceptTCPConnectionFailed(String),
    #[error("Failed to parse the request body, received error: `{0}`")]
    ParseBodyFailed(String),
    #[error("Failed to parse the response body, received error: `{0}`")]
    ParseResponseBodyFailed(String),
}

use AntelopeFirewallError::*;

impl AntelopeFirewall {
    pub fn new(routing_mode: RoutingModeState, socket_addr: SocketAddr) -> Self {
        AntelopeFirewall {
            filters: Vec::new(),
            ratelimiters: Vec::new(),
            matching_engine: MatchingEngine::new(),
            routing_mode,
            socket_addr
        }
    }
    pub fn add_filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }
    pub fn add_ratelimiter(mut self, ratelimiter: RateLimiter) -> Self {
        self.ratelimiters.push(ratelimiter);
        self
    }
    pub fn add_matching_rule(mut self, rule: Box<MatchingFn>) -> Self {
        self.matching_engine.add_rule(rule);
        self
    }

    pub fn build(self) -> Arc<Self> {
        Arc::new(self)
    }
    pub async fn run(self: Arc<Self>) -> Result<(), AntelopeFirewallError> {
        let listener = TcpListener::bind(self.socket_addr)
            .await
            .map_err(|e| AntelopeFirewallError::StartingServerFailed(e.to_string(), self.socket_addr))?;

        // TODO: Start Prometheus

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| AntelopeFirewallError::AcceptTCPConnectionFailed(e.to_string()))?;

            let new_self = Arc::clone(&self);
            tokio::task::spawn(async move {
                let address = stream
                    .peer_addr()
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

    async fn handle_request(
        &self,
        req: Request<hyper::body::Incoming>,
        ip: IpAddr,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, AntelopeFirewallError> {
        REQUESTS_RECEIVED.inc();

        // Parse thr request, try to put body into JSON
        let (parts, body) = req.into_parts();

        // Check size hint, return 413 error if too big
        let max = body.size_hint().upper().unwrap_or(u64::MAX);
        // TODO: make this configurable
        if max > 1024 * 64 {
            let mut resp = Response::new(full("Body too big"));
            *resp.status_mut() = hyper::StatusCode::PAYLOAD_TOO_LARGE;
            return Ok(resp);
        }

        let request_info = Arc::new(RequestInfo::new(parts.headers.clone(), parts.uri.clone(), ip));
        let body_bytes = match parts.method {
            Method::POST => body
                .collect()
                .await
                .map_err(|e| ParseBodyFailed(e.to_string()))?
                .to_bytes(),
            _ => Bytes::new(),
        };
        
        let body_json = Arc::new(
            match parts.method {
                Method::POST => {
                    let mut parsed = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                        .map_err(|e| ParseBodyFailed(e.to_string()))?;
                    if let Some(m) = parsed.as_object_mut() {
                        if let Some(hex) = m.get_mut("packed_trx")
                            .and_then(|e| e.as_str())
                            .and_then(|s| hex::decode(s).ok()) {
                            if let Some(serialized) = crate::de::from_bytes::<Transaction>(&hex[..]).ok()
                                .and_then(|trx| serde_json::to_value(&trx).ok()) {
                                m.insert("unpacked_trx".into(), serialized);
                            }
                        }
                    }
                    parsed
                },
                _ => serde_json::Value::Null
            }
        );

        println!("Checking filter");
        // Check if the request should be filtered out
        for filter in &self.filters {
            if !filter
                .should_request_pass(Arc::clone(&request_info), Arc::clone(&body_json))
                .await
            {
                return Ok(get_blocked_response());
            }
        }

        if parts.method == Method::OPTIONS {
            return Ok(get_options_response());
        }

        println!("Checking rate limit");
        // Check if the request should be rate limited
        for ratelimiter in &self.ratelimiters {
            if !ratelimiter
                .should_request_pass(Arc::clone(&request_info), Arc::clone(&body_json))
                .await
            {
                println!("Request failed on {}", ratelimiter.name);
                return Ok(get_ratelimit_response(ratelimiter.get_window_duration()));
            }
        }

        println!("Finding end url");
        // Find end nodes that can accept the request with the matching engine
        let urls = self
            .matching_engine
            .find_matching_urls(Arc::clone(&request_info), Arc::clone(&body_json))
            .await;
        if urls.len() == 0 {
            REQUESTS_FAILED_TO_ROUTE.inc();
            return Ok(get_error_response(full(
                "Failed to find a route for your request.",
            )));
        }

        let mut url = match self.routing_mode {
            RoutingModeState::LeastConnected(ref counts) => {
                urls.into_iter()
                    .map(|(url, weight)| {
                        (
                            url.clone(),
                            counts
                                .get(&url.host().unwrap().to_string())
                                .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
                                .unwrap_or(1) as f32
                                / weight as f32,
                        )
                    })
                    .min_by(|(_, w1), (_, w2)| {
                        w1.partial_cmp(w2).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .expect("There were no possible urls even though it was checked earlier")
                    .0
            }
            RoutingModeState::RoundRobin(ref counts) => {
                let count = counts
                    .get(request_info.uri.path())
                    .map(|a| a.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
                    .unwrap_or(0);

                let modulated = count % urls.iter().map(|(_, weight)| weight).sum::<u64>();
                urls.iter()
                    .fold_while(
                        (modulated, Url::parse("https://127.0.0.1/").unwrap()),
                        |(weights_left, s), (url, weight)| {
                            if weights_left < *weight {
                                Done((0, url.clone()))
                            } else {
                                Continue((weights_left.checked_sub(*weight).unwrap_or(0), s))
                            }
                        },
                    )
                    .into_inner()
                    .1
            }
            RoutingModeState::Random => {
                let (urls, weights): (Vec<Url>, Vec<u64>) = urls.into_iter().unzip();
                let dist = WeightedIndex::new(weights).unwrap();
                urls[dist.sample(&mut rand::thread_rng())].clone()
            }
        };

        let prometheus_url_name = url.host_str().unwrap_or("unknown").replace(|c: char| c != '_' && !c.is_alphanumeric(), "_");
        {
            let guard = MATCHING_COUNTERS.read().await;
            if !guard.contains_key(&prometheus_url_name) {
                drop(guard);
                let processed_counter = register_counter!(
                    format!("node_{}_processed", prometheus_url_name),
                    format!("Number of requests sent to node {}", prometheus_url_name)
                ).unwrap();
                let success_counter = register_counter!(
                    format!("node_{}_success", prometheus_url_name),
                    format!("Number of requests sent to node {} that returned success", prometheus_url_name)
                ).unwrap();
                let client_error_counter = register_counter!(
                    format!("node_{}_client_error", prometheus_url_name),
                    format!("Number of requests sent to node {} that returned a 4** error", prometheus_url_name)
                ).unwrap();
                let server_error_counter = register_counter!(
                    format!("node_{}_server_error", prometheus_url_name),
                    format!("Number of requests sent to node {} that returned a 5** error", prometheus_url_name)
                ).unwrap();
                processed_counter.inc();

                let mut guard = MATCHING_COUNTERS.write().await;
                guard.insert(
                    prometheus_url_name.clone(),
                    (processed_counter, success_counter, client_error_counter, server_error_counter)
                );
            } else {
                guard.get(&prometheus_url_name).unwrap().0.inc();
            }
        }

        url.set_path(&parts.uri.to_string());

        // Send the request
        let mut headers = parts.headers;
        headers.insert("X-Forwarded-For", ip.to_string().parse().unwrap());

        println!("Sending to url: {:?}", url);
        let client = reqwest::Client::new();
        let node_res = client
            .post(url.clone())
            .headers(headers)
            .body(body_bytes)
            .send()
            .await
            .unwrap();
        let node_status = node_res.status();

        {
            let guard = MATCHING_COUNTERS.read().await;
            let (_, success_counter, client_error_counter, server_error_counter) = guard.get(&prometheus_url_name).unwrap();
            if node_status.is_success() {
                SUCCESS_NODE_RESPONSES.inc();
                success_counter.inc();
            } else if node_status.is_client_error() {
                CLIENT_ERROR_NODE_RESPONSES.inc();
                client_error_counter.inc();
            } else if node_status.is_server_error() {
                SERVER_ERROR_NODE_RESPONSES.inc();
                server_error_counter.inc();
            }
        }

        // Respond to the client
        let mut client_res = Response::builder().status(node_res.status());
        client_res
            .headers_mut()
            .map(|h| h.clone_from(node_res.headers()));

        let response_bytes = node_res.bytes().await.unwrap();
        let response_json = Arc::new(
            (
                serde_json::from_slice::<serde_json::Value>(&response_bytes)
                    .map_err(|e| ParseResponseBodyFailed(e.to_string()))?,
                node_status
            )
        );

        // Update any ratelimiters that need to be notified on failure
        for ratelimiter in &self.ratelimiters {
            if !ratelimiter.increment_mode.should_run_before_request() {
                ratelimiter
                    .post_increment(
                        Arc::clone(&request_info),
                        Arc::clone(&body_json),
                        Arc::clone(&response_json),
                    )
                    .await;
            }
        }

        let final_response = client_res.body(full(response_bytes)).unwrap();
        Ok(final_response)
    }
}
