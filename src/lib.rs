use std::net::IpAddr;

use hyper::{HeaderMap, Uri};
use serde_json::Value;

mod firewall_builder;

mod filter;
mod ratelimiter;
mod json_data_cache;
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

type FilterFn = dyn Fn((&RequestInfo, &Value, &Value)) -> bool + Send + Sync;
type MapFn<T> = dyn Fn((&RequestInfo, &Value, &Value)) -> T + Send + Sync;
