use serde_json::Value;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

use crate::{json_data_cache::JsonDataCache, FilterFn, MapFn, PostMapFn, RequestInfo};

pub enum IncrementMode {
    Before(Box<MapFn<u64>>),
    After(Box<PostMapFn<u64>>),
}

impl IncrementMode {
    pub fn should_run_before_request(&self) -> bool {
        match self {
            IncrementMode::Before(_) => true,
            IncrementMode::After(_) => false,
        }
    }
}

/// RateLimiter is a struct that will be used to limit the rate of requests.
/// We use the SlidingWindow technique, where we maintain two buckets
pub struct RateLimiter<T> {
    pub name: String,

    should_be_limited: Box<FilterFn>,
    get_bucket: Box<MapFn<Option<T>>>,
    get_ratelimit: Box<MapFn<u64>>,

    pub increment_mode: IncrementMode,

    cache: Option<Arc<JsonDataCache>>,

    window_duration: u64,

    current_window_state: Mutex<(u64, HashMap<T, u64>, HashMap<T, u64>)>,
}

impl<T: Eq + Hash + Clone> RateLimiter<T> {
    pub fn new(
        name: String,
        should_be_limited: Box<FilterFn>,
        get_bucket: Box<MapFn<Option<T>>>,
        get_ratelimit: Box<MapFn<u64>>,
        increment_mode: IncrementMode,
        cache: Option<Arc<JsonDataCache>>,
        window_duration: u64,
    ) -> Self {
        RateLimiter {
            name,
            should_be_limited,
            get_bucket,
            get_ratelimit,
            increment_mode,
            cache,
            window_duration,
            current_window_state: Mutex::new((
                RateLimiter::<T>::get_current_window(window_duration),
                HashMap::new(),
                HashMap::new(),
            )),
        }
    }

    pub fn get_current_window(window_duration: u64) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            / window_duration
    }

    fn elapsed_current_window(window_duration: u64, current_window: u64) -> f32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH + Duration::from_secs(current_window * window_duration))
            .expect("Time went backwards")
            .as_secs_f32()
            / window_duration as f32
    }

    pub fn get_window_duration(&self) -> u64 {
        self.window_duration
    }

    pub fn update_current_window(
        window_duration: u64,
        current_window: &mut u64,
        last_buckets: &mut HashMap<T, u64>,
        current_buckets: &mut HashMap<T, u64>,
    ) {
        let real_current_window = RateLimiter::<T>::get_current_window(window_duration);
        if real_current_window != *current_window {
            std::mem::swap(last_buckets, current_buckets);
            *current_buckets = HashMap::new();

            if *current_window + 1 != real_current_window {
                *last_buckets = HashMap::new();
            }
            *current_window = real_current_window;
        }
    }

    pub async fn should_request_pass(
        &self,
        request_info: Arc<RequestInfo>,
        value: Arc<Value>,
    ) -> bool {
        let (current_window, last_buckets, current_buckets) =
            &mut *self.current_window_state.lock().await;

        Self::update_current_window(
            self.window_duration,
            current_window,
            last_buckets,
            current_buckets,
        );

        let json = match JsonDataCache::handle_cache_option(&self.cache).await {
            Some(val) => val,
            None => {
                return false;
            }
        };
        let (bucket, rate_limit) = if (self.should_be_limited)((
            Arc::clone(&request_info),
            Arc::clone(&value),
            Arc::clone(&json),
        ))
        .await
        {
            (
                (self.get_bucket)((
                    Arc::clone(&request_info),
                    Arc::clone(&value),
                    Arc::clone(&json),
                ))
                .await,
                (self.get_ratelimit)((
                    Arc::clone(&request_info),
                    Arc::clone(&value),
                    Arc::clone(&json),
                ))
                .await,
            )
        } else {
            return false;
        };

        let should_pass = match bucket {
            Some(ref bucket) => {
                let current_count = *current_buckets.get(bucket).unwrap_or(&0);
                let last_count = *last_buckets.get(bucket).unwrap_or(&0);

                let ratio_elapsed =
                    RateLimiter::<T>::elapsed_current_window(self.window_duration, *current_window);
                let count_for_ip = ((current_count as f32) * ratio_elapsed)
                    + (last_count as f32 * (1.0 - ratio_elapsed));

                if let IncrementMode::Before(get_rate_used) = &self.increment_mode {
                    Self::increment(
                        current_buckets,
                        bucket.clone(),
                        get_rate_used((request_info, value, json)).await,
                    )
                    .await;
                };

                count_for_ip > rate_limit as f32 && current_count <= (2 * rate_limit)
            }
            None => false,
        };

        should_pass
    }

    pub async fn increment(current_buckets: &mut HashMap<T, u64>, bucket: T, val: u64) {
        current_buckets
            .entry(bucket)
            .and_modify(|e| *e += val)
            .or_insert(val);
    }

    pub async fn post_increment(
        &self,
        request_info: Arc<RequestInfo>,
        data: Arc<Value>,
        response: Arc<Value>,
    ) {
        let (_, _, current_buckets) = &mut *self.current_window_state.lock().await;
        let json = match JsonDataCache::handle_cache_option(&self.cache).await {
            Some(val) => val,
            None => {
                return;
            }
        };
        if let IncrementMode::After(get_rate_used) = &self.increment_mode {
            if let Some(bucket) = (self.get_bucket)((
                Arc::clone(&request_info),
                Arc::clone(&data),
                Arc::clone(&json),
            ))
            .await
            {
                Self::increment(
                    current_buckets,
                    bucket,
                    get_rate_used((request_info, data, response, json)).await,
                )
                .await;
            }
        }
    }
}
