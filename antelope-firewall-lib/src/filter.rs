use std::sync::Arc;

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
}

impl Filter {
    pub fn new(
        name: String,
        should_request_pass: Box<FilterFn>,
        cache: Option<Arc<JsonDataCache>>,
    ) -> Self {
        Filter {
            name,
            should_request_pass,
            cache,
        }
    }

    /// Checks if the request should be filtered out
    pub async fn should_request_pass(
        &self,
        request_info: Arc<RequestInfo>,
        value: Arc<Value>,
    ) -> bool {
        let json = match JsonDataCache::handle_cache_option(&self.cache).await {
            Some(val) => val,
            None => {
                return false;
            }
        };
        (self.should_request_pass)((request_info, value, json)).await
    }
}
