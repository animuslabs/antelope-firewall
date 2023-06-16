use reqwest::Url;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Interval};

/// Periodically fetches data from external sources and caches it in memory.
pub struct JsonDataCache {
    pub data: Arc<RwLock<(serde_json::Value, JsonDataCacheStatus)>>,
}

#[derive(Debug, Clone)]
pub enum JsonDataCacheStatus {
    Ok,
    Uninitialized,
    UnableToFetch(String),
    InvalidResponse(String)
}

impl JsonDataCache {
    /// Create new JsonDataCache with specified url and interval
    pub fn new(url: Url, mut interval: Interval) -> Self {
        let cache = JsonDataCache {
            data: Arc::new(RwLock::new((
              serde_json::Value::Null, JsonDataCacheStatus::Uninitialized
            ))),
        };

        let cache_data = Arc::clone(&cache.data);
        
        // Every interval, fetch the data from the URL and cache it in memory.
        tokio::task::spawn(async move {
            interval.tick().await;

            // TODO: Integrate with Prometheus
            // TODO: Log errors
            let client = reqwest::Client::new();
            loop {
                let response = client.get(url.clone()).send().await;
                match response {
                    Ok(response) => {
                        let status = response.status();
                        if status.is_success() {
                            let json = response.json::<serde_json::Value>().await;
                            match json {
                                Ok(json) => {
                                    let mut data = cache_data.write().await;
                                    *data = (json, JsonDataCacheStatus::Ok);
                                },
                                Err(e) => {
                                    let mut data = cache_data.write().await;
                                    data.1 = JsonDataCacheStatus::InvalidResponse(e.to_string());
                                }
                            }
                        } else {
                            let mut data = cache_data.write().await;
                            data.1 = JsonDataCacheStatus::UnableToFetch(status.to_string());
                        }
                    },
                    Err(e) => {
                        let mut data = cache_data.write().await;
                        data.1 = JsonDataCacheStatus::UnableToFetch(e.to_string());
                    }
                }
            }
        });
        cache
    }
}
