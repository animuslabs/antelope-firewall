use std::{net::IpAddr, collections::HashSet, pin::Pin, future::Future, sync::Arc};

use hyper::{HeaderMap, Uri};
use reqwest::Url;
use serde_json::Value;

pub mod firewall_builder;

pub mod filter;
pub mod ratelimiter;
pub mod json_data_cache;
pub mod matching_engine;
pub mod healthcheck;
pub mod api_responses;

mod util;

#[derive(Debug)]
pub struct RequestInfo {
    headers: HeaderMap,
    uri: Uri,
    ip: IpAddr
}

impl RequestInfo {
    pub fn new(headers: HeaderMap, uri: Uri, ip: IpAddr) -> Self {
        RequestInfo {
            headers,
            uri,
            ip
        }
    }
}

pub type Fut<T> = Pin<Box<dyn Future<Output = T> + Send + Sync>>;

pub type FilterFn = dyn Fn((Arc<RequestInfo>, Arc<Value>, Arc<Value>)) -> Fut<bool> + Send + Sync + 'static;
pub type MapFn<T> = dyn Fn((Arc<RequestInfo>, Arc<Value>, Arc<Value>)) -> Fut<T> + Send + Sync + 'static;
pub type MatchingFn = dyn Fn((Arc<RequestInfo>, Arc<Value>, Arc<Value>, HashSet<(Url, u64)>)) -> Fut<HashSet<(Url, u64)>> + Send + Sync + 'static;