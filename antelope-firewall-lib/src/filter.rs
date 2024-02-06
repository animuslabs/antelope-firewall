use std::sync::Arc;

use prometheus_exporter::prometheus::{core::{AtomicF64, GenericCounter}, register_counter};
use serde_json::Value;

use crate::{
    json_data_cache::{JsonDataCache, JsonDataCacheStatus},
    FilterFn, RequestInfo,
};

/// A filter is applied in the first step of the pipeline, and simply
/// decides whether or not a request should carry through
pub struct Filter {
    name: String,
    should_request_pass: Box<FilterFn>,
    cache: Option<Arc<JsonDataCache>>,
    processed_counter: GenericCounter<AtomicF64>,
    denied_counter: GenericCounter<AtomicF64>,
    accepted_counter: GenericCounter<AtomicF64>
}

impl Filter {
    pub fn new(
        name: String,
        should_request_pass: Box<FilterFn>,
        cache: Option<Arc<JsonDataCache>>,
    ) -> Self {
        Filter {
            name: name.clone(),
            should_request_pass,
            cache,
            processed_counter: register_counter!(
                format!("filter_{}_processed", name),
                format!("Number of requests processed by {} filter", name),
            ).unwrap(),
            denied_counter: register_counter!(
                format!("filter_{}_denied", name),
                format!("Number of requests denied by {} filter", name),
            ).unwrap(),
            accepted_counter: register_counter!(
                format!("filter_{}_accepted", name),
                format!("Number of requests accepted by {} filter", name),
            ).unwrap(),
        }
    }

    /// Checks if the request should be filtered out
    pub async fn should_request_pass(
        &self,
        request_info: Arc<RequestInfo>,
        value: Arc<Value>,
    ) -> bool {
        self.processed_counter.inc();
        let json = match JsonDataCache::handle_cache_option(&self.cache).await {
            Some(val) => val,
            None => {
                return false;
            }
        };
        let retval = (self.should_request_pass)((request_info, value, json)).await;
        if retval {
            self.accepted_counter.inc();
        } else {
            self.denied_counter.inc();
        }
        retval
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use hyper::{HeaderMap, Uri};
    use serde_json::Map;

    use super::*;

    #[tokio::test]
    async fn allows_requests_through() {
        let filter = Filter::new(
            "passes_all".into(), 
            Box::new(|_| Box::pin(async { true })),
            None
        );
        let should_pass = filter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(should_pass, true);
    }

    #[tokio::test]
    async fn denies_requests() {
        let filter = Filter::new(
            "denies_all".into(), 
            Box::new(|_| Box::pin(async { false })),
            None
        );
        let should_pass = filter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(should_pass, false);
    }

    #[tokio::test]
    async fn passes_requestinfo_to_function() {
        let filter = Filter::new(
            "passes_requestinfo".into(), 
            Box::new(|(a, _, _)| Box::pin(async move {
                a.headers
                    .get("accept")
                    .and_then(|r| r.to_str().ok().map(|s| s == "application/json"))
                    .unwrap_or(false)
            })),
            None
        );

        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/json".parse().unwrap());
        let should_pass = filter.should_request_pass(
            Arc::new(RequestInfo::new(
                headers,
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(should_pass, true);

        let mut headers_two = HeaderMap::new();
        headers_two.insert("accept", "text".parse().unwrap());
        let should_pass = filter.should_request_pass(
            Arc::new(RequestInfo::new(
                headers_two,
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(should_pass, false);
    }

    #[tokio::test]
    async fn passes_value_to_function() {
        let filter = Filter::new(
            "passes_value".into(), 
            Box::new(|(_, a, _)| Box::pin(async move {
                a.is_null()
            })),
            None
        );

        let should_pass = filter.should_request_pass(
            Arc::new(RequestInfo::new(
                HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Null)
        ).await;
        assert_eq!(should_pass, true);

        let should_pass = filter.should_request_pass(
            Arc::new(RequestInfo::new(
                HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(should_pass, false);
    }
}
