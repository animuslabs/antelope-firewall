use hyper::StatusCode;
use prometheus_exporter::prometheus::{core::{GenericCounter, AtomicF64}, register_counter};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::Arc,
    fmt::Debug
};
use tokio::sync::Mutex;

#[cfg(test)]
use mock_instant::{SystemTime, UNIX_EPOCH};

#[cfg(not(test))]
use std::time::{SystemTime, UNIX_EPOCH};

use std::time::Duration;

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

    /// Fraction of the current window that is elapsed.
    /// Only valid if get_current_window was called immediately before
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

    fn update_current_window(
        window_duration: u64,
        current_window: &mut u64,
        last_buckets: &mut HashMap<String, u64>,
        current_buckets: &mut HashMap<String, u64>,
    ) {
        let real_current_window = RateLimiter::get_current_window(window_duration);
        if real_current_window != *current_window {
            // clear buckets for the current window
            std::mem::swap(last_buckets, current_buckets);
            *current_buckets = HashMap::new();

            // If the current window is not immediately after the last window,
            // clear buckets for last window
            if *current_window + 1 != real_current_window {
                *last_buckets = HashMap::new();
            }
            *current_window = real_current_window;
        }
    }

    /// Takes a Request Info struct that contains info about the request, and the
    /// request body as a value, and returns whether or not the request will be let
    /// through the ratelimiter. Does not take self as mut but does thread-safely
    /// increments the rate limiter buckets
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

    async fn increment(current_buckets: &mut HashMap<String, u64>, bucket: String, val: u64) {
        current_buckets
            .entry(bucket)
            .and_modify(|e| *e += val)
            .or_insert(val);
    }

    /// Run after the request has been sent to trigger an increment on the ratelimiter
    /// Takes the RequestInfo struct and JSON request body, along with response
    /// Useful so that increment logic can be made run based on logic of response
    /// 
    /// Does not take self as mut but can modify the ratelimiter in a thread-safe way
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

#[cfg(test)]
mod tests {

    use std::net::{IpAddr, Ipv4Addr};

    use hyper::{HeaderMap, Uri};
    use serde_json::Map;
    use mock_instant::MockClock;

    use super::*;

    #[tokio::test]
    async fn create() {
        let limiter = RateLimiter::new(
            "create".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 3 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, true);
    }

    #[tokio::test]
    async fn allows_appropriate_window_overflow() {
        MockClock::set_system_time(Duration::from_secs(5));
        let limiter = RateLimiter::new(
            "overflow1".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 100 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );

        for _ in 0..200 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
        }

        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn blocks_excess_overflow_accross_windows() {
        MockClock::set_system_time(Duration::from_secs(5));
        let limiter = RateLimiter::new(
            "overflow2".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 100 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );

        for _ in 0..200 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
        }

        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);

        MockClock::set_system_time(Duration::from_secs(15));
        println!("System time: 15s");
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn allows_appropriate_overflow_accross_windows() {
        MockClock::set_system_time(Duration::from_secs(5));
        let limiter = RateLimiter::new(
            "overflow3".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 100 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );

        for _ in 0..120 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
        }

        MockClock::set_system_time(Duration::from_secs(12));
        for _ in 0..21 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
        }

        // 120 * 0.8 + 20 * 0.2 = 100 
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn handles_sporadic_requests() {
        MockClock::set_system_time(Duration::from_secs(0));
        let limiter = RateLimiter::new(
            "sporadic".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 100 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );

        for _ in 0..200 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
        }

        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);

        MockClock::set_system_time(Duration::from_secs(100));
        for _ in 0..200 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
        }

        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn has_proper_window_duration() {
        let limiter = RateLimiter::new(
            "window_duration".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 100 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            140
        );
        assert_eq!(limiter.get_window_duration(), 140);
    }

    #[tokio::test]
    async fn post_increment_works() {
        MockClock::set_system_time(Duration::from_secs(100));
        let limiter = RateLimiter::new(
            "post_increment".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 100 })),
            IncrementMode::After(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );

        for _ in 0..200 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);
            limiter.post_increment(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new())),
                Arc::new((Value::Object(Map::new()), StatusCode::ACCEPTED))
            ).await;
        }

        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn should_pass_and_post_increment_should_not_both_call_before() {
        MockClock::set_system_time(Duration::from_secs(15));
        let limiter = RateLimiter::new(
            "before".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 1 })),
            IncrementMode::Before(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, true);

        limiter.post_increment(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new())),
            Arc::new((Value::Object(Map::new()), StatusCode::ACCEPTED))
        ).await;

        // second call passes because overflow
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, true);

        limiter.post_increment(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new())),
            Arc::new((Value::Object(Map::new()), StatusCode::ACCEPTED))
        ).await;

        // third call should not pass
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn should_pass_and_post_increment_should_not_both_call_after() {
        MockClock::set_system_time(Duration::from_secs(15));
        let limiter = RateLimiter::new(
            "after".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
            })),
            Box::new(move |_| Box::pin(async move { 1 })),
            IncrementMode::After(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, true);

        limiter.post_increment(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new())),
            Arc::new((Value::Object(Map::new()), StatusCode::ACCEPTED))
        ).await;

        // second call passes because overflow
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, true);

        limiter.post_increment(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new())),
            Arc::new((Value::Object(Map::new()), StatusCode::ACCEPTED))
        ).await;

        // third call should not pass
        let passed = limiter.should_request_pass(
            Arc::new(RequestInfo::new(HeaderMap::new(),
                Uri::from_static("https://google.com/"),
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
            )),
            Arc::new(Value::Object(Map::new()))
        ).await;
        assert_eq!(passed, false);
    }

    #[tokio::test]
    async fn all_pass_with_no_buckets() {
        let limiter = RateLimiter::new(
            "empty".into(),
            Box::new(|_| Box::pin(async { true })),
            Box::new(|_| Box::pin(async move {
                HashSet::from([])
            })),
            Box::new(move |_| Box::pin(async move { 10 })),
            IncrementMode::After(Box::new(|_| Box::pin(async { 1 }))),
            None,
            10
        );
        
        for _ in 1..100 {
            let passed = limiter.should_request_pass(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new()))
            ).await;
            assert_eq!(passed, true);

            limiter.post_increment(
                Arc::new(RequestInfo::new(HeaderMap::new(),
                    Uri::from_static("https://google.com/"),
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
                )),
                Arc::new(Value::Object(Map::new())),
                Arc::new((Value::Object(Map::new()), StatusCode::ACCEPTED))
            ).await;
        }
    }
}
