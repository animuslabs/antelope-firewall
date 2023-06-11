use std::{collections::HashMap, hash::Hash, sync::Arc, time::{SystemTime, UNIX_EPOCH, Duration}};
use serde_json::Value;

use hyper::http::request::Parts;

use crate::{FilterFn, json_data_cache::{JsonDataCache, JsonDataCacheStatus}, MapFn, RequestInfo};


/// RateLimiter is a struct that will be used to limit the rate of requests.
/// We use the SlidingWindow technique, where we maintain two buckets
pub struct RateLimiter<T> {
    name: String,

    should_be_limited: Box<FilterFn>,
    get_bucket: Box<MapFn<Option<T>>>,
    get_ratelimit: Box<MapFn<u64>>,

    cache: Option<Arc<JsonDataCache>>,

    window_duration: u64,
    current_window: u64,

    current_buckets: Arc<HashMap<T, u64>>,
    last_buckets: Arc<HashMap<T, u64>>
}

impl<T: Eq + Hash> RateLimiter<T> {
    pub fn new(
        name: String,
        should_be_limited: Box<FilterFn>,
        get_bucket: Box<MapFn<Option<T>>>,
        get_ratelimit: Box<MapFn<u64>>,
        cache: Option<Arc<JsonDataCache>>,
        window_duration: u64,
    ) -> Self {
        let mut limiter = RateLimiter {
            name,
            should_be_limited,
            get_bucket,
            get_ratelimit,
            cache,
            window_duration,
            current_window: 0,
            current_buckets: Arc::new(HashMap::new()),
            last_buckets: Arc::new(HashMap::new())
        };
        limiter.current_window = limiter.get_current_window();
        limiter
    }

    pub fn get_current_window(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards").as_secs() / self.window_duration
    }

    pub fn elapsed_current_window(&self) -> f32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH + Duration::from_secs(self.current_window * self.window_duration))
            .expect("Time went backwards").as_secs_f32() / self.window_duration as f32
    }

    pub fn get_window_duration(&self) -> u64 {
        self.window_duration
    }

    pub fn update_current_window(&mut self) {
        let current_window = self.get_current_window();
        if current_window != self.current_window {
            self.last_buckets = Arc::clone(&self.current_buckets);
            self.current_buckets = Arc::new(HashMap::new());
            
            if self.current_window + 1 != current_window {
                self.last_buckets = Arc::new(HashMap::new());
            }
            self.current_window = current_window;
        }
    }

    pub async fn should_request_pass(&mut self, request_info: &RequestInfo, value: &Value) -> bool {
        self.update_current_window();

        let (bucket, rate_limit) = match self.cache {
            Some(ref cache) => {
                let data = cache.data.read().await;
                let (json, status) = &*data;
                match status {
                    JsonDataCacheStatus::Ok => {
                        self.get_parameters(request_info, value, json)
                    },
                    _ => {
                        return false;
                    }
                }
            },
            None => {
                self.get_parameters(request_info, value, &Value::Null)
            }
        };
        
        match bucket {
            Some(ref bucket) => {
                let current_count = *self.current_buckets.get(bucket).unwrap_or(&0);
                let last_count = *self.last_buckets.get(bucket).unwrap_or(&0);

                let ratio_elapsed = self.elapsed_current_window();
                let count_for_ip = ((current_count as f32) * ratio_elapsed) + (
                    last_count as f32 * (1.0 - ratio_elapsed)
                );

                count_for_ip > rate_limit as f32 && current_count <= (2 * rate_limit)
            },
            None => {
                false
            }
        }
    }

    // TODO: Add increment functions

    fn get_parameters(&self, request_info: &RequestInfo, value: &Value, data: &Value) -> (Option<T>, u64) {
        let bucket = (self.get_bucket)((request_info, value, data));
        let rate_limit = (self.get_ratelimit)((request_info, value, data));
        (bucket, rate_limit)
    }
}
