use hyper::StatusCode;
use prometheus_exporter::prometheus::{core::{GenericCounter, AtomicF64}, register_counter};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH}, fmt::Debug,
};
use tokio::sync::Mutex;

use crate::{json_data_cache::JsonDataCache, FilterFn, MapFn, PostMapFn, RequestInfo, RatelimiterMapFn};

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
pub struct RateLimiter {
    pub name: String,

    should_be_limited: Box<FilterFn>,
    get_buckets: Box<RatelimiterMapFn<HashSet<String>>>,
    get_ratelimit: Box<MapFn<u64>>,

    pub increment_mode: IncrementMode,

    cache: Option<Arc<JsonDataCache>>,

    window_duration: u64,

    current_window_state: Mutex<(u64, HashMap<String, u64>, HashMap<String, u64>)>,
    processed_counter: GenericCounter<AtomicF64>,
    denied_counter: GenericCounter<AtomicF64>,
    accepted_counter: GenericCounter<AtomicF64>
}

impl RateLimiter {
    pub fn new(
        name: String,
        should_be_limited: Box<FilterFn>,
        get_buckets: Box<RatelimiterMapFn<HashSet<String>>>,
        get_ratelimit: Box<MapFn<u64>>,
        increment_mode: IncrementMode,
        cache: Option<Arc<JsonDataCache>>,
        window_duration: u64,
    ) -> Self {
        RateLimiter {
            name: name.clone(),
            should_be_limited,
            get_buckets,
            get_ratelimit,
            increment_mode,
            cache,
            window_duration,
            current_window_state: Mutex::new((
                RateLimiter::get_current_window(window_duration),
                HashMap::new(),
                HashMap::new(),
            )),
            processed_counter: register_counter!(format!("ratelimiter_{}_processed", name), format!("Number of requests processed by {} ratelimiter", name)).unwrap(),
            denied_counter: register_counter!(format!("ratelimiter_{}_denied", name), format!("Number of requests denied by {} ratelimiter", name)).unwrap(),
            accepted_counter: register_counter!(format!("ratelimiter_{}_accepted", name), format!("Number of requests accepted by {} ratelimiter", name)).unwrap(),
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
        last_buckets: &mut HashMap<String, u64>,
        current_buckets: &mut HashMap<String, u64>,
    ) {
        let real_current_window = RateLimiter::get_current_window(window_duration);
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
        self.processed_counter.inc();
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
        let (buckets, rate_limit) = if (self.should_be_limited)((
            Arc::clone(&request_info),
            Arc::clone(&value),
            Arc::clone(&json),
        ))
        .await
        {
            (
                (self.get_buckets)((
                    Arc::new(self.name.clone()),
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

        for ref bucket in buckets {
            let current_count = *current_buckets.get(bucket).unwrap_or(&0);
            let last_count = *last_buckets.get(bucket).unwrap_or(&0);

            let ratio_elapsed =
                RateLimiter::elapsed_current_window(self.window_duration, *current_window);
            let count_for_ip = ((current_count as f32) * ratio_elapsed)
                + (last_count as f32 * (1.0 - ratio_elapsed));

            if let IncrementMode::Before(get_rate_used) = &self.increment_mode {
                Self::increment(
                    current_buckets,
                    bucket.clone(),
                    get_rate_used((Arc::clone(&request_info), Arc::clone(&value), Arc::clone(&json))).await,
                )
                .await;
            };

            if (count_for_ip > rate_limit as f32) || current_count >= (2 * rate_limit) {
                self.denied_counter.inc();
                return false;
            }
        }
        self.accepted_counter.inc();
        true
    }

    pub async fn increment(current_buckets: &mut HashMap<String, u64>, bucket: String, val: u64) {
        current_buckets
            .entry(bucket)
            .and_modify(|e| *e += val)
            .or_insert(val);
    }

    pub async fn post_increment(
        &self,
        request_info: Arc<RequestInfo>,
        data: Arc<Value>,
        response: Arc<(Value, StatusCode)>,
    ) {
        let (_, _, current_buckets) = &mut *self.current_window_state.lock().await;
        let json = match JsonDataCache::handle_cache_option(&self.cache).await {
            Some(val) => val,
            None => {
                return;
            }
        };
        if let IncrementMode::After(get_rate_used) = &self.increment_mode {
            let rate_used = get_rate_used((
                Arc::clone(&request_info), 
                Arc::clone(&data),
                Arc::clone(&response),
                Arc::clone(&json)
            )).await;
            for bucket in (self.get_buckets)((
                Arc::new(self.name.clone()),
                Arc::clone(&request_info),
                Arc::clone(&data),
                Arc::clone(&json),
            ))
            .await
            {
                Self::increment(
                    current_buckets,
                    bucket,
                    rate_used
                )
                .await;
            }
        }
    }
}
