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
