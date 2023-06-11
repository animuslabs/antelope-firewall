use tokio::sync::Mutex;
use thiserror::Error;
use std::net::{SocketAddr, IpAddr};
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::{RequestInfo, ratelimiter};
use crate::util::{get_blocked_response, get_ratelimit_response};
use crate::{filter::Filter, ratelimiter::RateLimiter};

use hyper::server::conn::http1;
use hyper::{Request, Response, Method};
use hyper::service::service_fn;
use hyper::body::Bytes;

use http_body_util::combinators::BoxBody;
use http_body_util::{Full, BodyExt};

pub struct AntelopeFirewall {
    filters: Vec<Filter>,
    ratelimiters: Vec<Mutex<RateLimiter<String>>>
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
    pub fn new() -> Self {
        AntelopeFirewall {
            filters: Vec::new(),
            ratelimiters: Vec::new()
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
        let (parts, body) = req.into_parts();
        let request_info = RequestInfo::new(parts.headers, parts.uri, ip);
        let body_bytes = body.collect().await
            .map_err(|e| ParseBodyFailed(e.to_string()))?
            .to_bytes();
        let body_json = serde_json::from_slice::<serde_json::Value>(&body_bytes)
            .map_err(|e| ParseBodyFailed(e.to_string()))?;

        // Check if the request should be filtered out
        for filter in &self.filters {
            if !filter.should_request_pass(&request_info, &body_json).await {
                return Ok(get_blocked_response());
            }
        }

        // Check if the request should be rate limited
        for ratelimiter in &self.ratelimiters {
            let mut ratelimiter = ratelimiter.lock().await;
            if !ratelimiter.should_request_pass(&request_info, &body_json).await {
                return Ok(get_ratelimit_response(ratelimiter.get_window_duration()));
            }
        }
        
        // Find an end node that can accept the request with the matching engine
        
        // Send the request
        
        // Update any ratelimiters that need to be notified on failure
        
        // Respond to the client

        todo!();
    }
}

pub async fn test() {
    let result = AntelopeFirewall::new()
        .add_filter(Filter::new("hello".into(), Box::new(|_| true), None))
        .add_ratelimiter(
            RateLimiter::new(
                "Base Ratelimiter".into(),
                Box::new(|_| true),
                Box::new(|(req, _, _)| Some(req.ip.to_string())),
                Box::new(|_| 10),
                None,
                10
            ))
        .build()
        .run().await;
}
