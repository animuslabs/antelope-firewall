use std::sync::Arc;

use serde_json::Value;

use crate::{json_data_cache::{JsonDataCache, JsonDataCacheStatus}, FilterFn, RequestInfo};

/// A filter is applied in the first step of the pipeline, and simply
/// decides whether or not a request should carry through
pub struct Filter {
    name: String,
    should_request_pass: Box<FilterFn>,
    cache: Option<Arc<JsonDataCache>>
}

impl Filter {
    pub fn new(
        name: String,
        should_request_pass: Box<FilterFn>,
        cache: Option<Arc<JsonDataCache>>
    ) -> Self {
        Filter {
            name,
            should_request_pass,
            cache
        }
    }

    /// Checks if the request should be filtered out
    pub async fn should_request_pass(&self, request_info: Arc<RequestInfo>, value: Arc<Value>) -> bool {
        match self.cache {
            Some(ref cache) => {
                let data = cache.data.read().await;
                let (json, status) = (*data).clone();
                match status {
                    JsonDataCacheStatus::Ok => {
                        return (self.should_request_pass)((request_info, value, Arc::new(json))).await;
                    },
                    _ => {
                        return false;
                    }
                }
            },
            None => {
                return (self.should_request_pass)((request_info, value, Arc::new(Value::Null))).await;
            }
        }
    }
}

