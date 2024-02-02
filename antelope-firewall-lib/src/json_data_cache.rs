use log::error;
use reqwest::Url;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::Interval};

/// Periodically fetches data from external sources and caches it in memory.
#[derive(Debug)]
pub struct JsonDataCache {
    pub data: Arc<RwLock<(Arc<serde_json::Value>, JsonDataCacheStatus)>>,
}

#[derive(Debug, Clone)]
pub enum JsonDataCacheStatus {
    Ok,
    Uninitialized,
    UnableToFetch(String),
    InvalidResponse(String),
}

impl JsonDataCache {
    /// Create new JsonDataCache with specified url and interval
    pub fn new(url: Url, mut interval: Interval) -> Self {
        let cache = JsonDataCache {
            data: Arc::new(RwLock::new((
                Arc::new(serde_json::Value::Null),
                JsonDataCacheStatus::Uninitialized,
            ))),
        };

        let cache_data = Arc::clone(&cache.data);

        // Every interval, fetch the data from the URL and cache it in memory.
        tokio::spawn(async move {
            interval.tick().await;

            // TODO: Integrate with Prometheus
            // TODO: Log errors
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build().expect("Error building reqwest client");
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
                                    *data = (Arc::new(json), JsonDataCacheStatus::Ok);
                                }
                                Err(e) => {
                                    let mut data = cache_data.write().await;
                                    data.1 = JsonDataCacheStatus::InvalidResponse(e.to_string());
                                }
                            }
                        } else {
                            let mut data = cache_data.write().await;
                            data.1 = JsonDataCacheStatus::UnableToFetch(status.to_string());
                        }
                    }
                    Err(e) => {
                        let mut data = cache_data.write().await;
                        data.1 = JsonDataCacheStatus::UnableToFetch(e.to_string());
                    }
                }
            }
        });
        cache
    }

    pub async fn handle_cache_option(cache_opt: &Option<Arc<Self>>) -> Option<Arc<Value>> {
        match cache_opt {
            Some(ref cache) => {
                println!("a");
                let data = cache.data.read().await;
                let (ref json, ref status) = *data;
                match status {
                    JsonDataCacheStatus::Ok => {
                        Some(Arc::clone(json))
                    },
                    JsonDataCacheStatus::UnableToFetch(e) => {
                        None
                    },
                    JsonDataCacheStatus::InvalidResponse(e) => {
                        None
                    },
                    JsonDataCacheStatus::Uninitialized => {
                        None
                    }
                }
            }
            None => {
                Some(Arc::new(Value::Null))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{thread::current, time::Duration};

    use super::*;

    #[tokio::test(start_paused=true)]
    async fn caches_value() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{\"hello\":\"world\"}")
            .create();
        let url = server.host_with_port();

        let data_cache = Some(Arc::new(JsonDataCache::new(
            format!("http://{}/", url).parse().unwrap(),
            tokio::time::interval(Duration::from_secs(10))
        )));
        println!("{:?}", data_cache);

        // Clear the task queue to simulate waiting for the value to be grabbed
        for _ in 0..1000 {
            println!("{:?}", data_cache);
            let current_value = JsonDataCache::handle_cache_option(&data_cache).await;
            
            match current_value {
                Some(json_value) => {
                    match json_value.as_ref() {
                        serde_json::Value::Object(m) => {
                            let y = m.get("hello");
                            assert_eq!(y.expect("Key doesnt exist"), "world")
                        },
                        serde_json::Value::Null => {
                            panic!("null")
                        },
                        _ => {
                            panic!("Not an object")
                        },
                    }
                    return
                },
                _ => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                },
            }
        }
    }

    #[tokio::test(start_paused=true)]
    async fn handles_uninitialized() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{\"hello\":\"world\"}")
            .create();
        let url = server.host_with_port();

        let data_cache = Some(Arc::new(JsonDataCache::new(
            format!("http://{}/", url).parse().unwrap(),
            tokio::time::interval(Duration::from_secs(10))
        )));

        let current_value = JsonDataCache::handle_cache_option(&data_cache).await;
        
        assert_eq!(current_value.is_none(), true);
    }

    #[tokio::test(start_paused=true)]
    async fn handles_server_error() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/")
            .with_status(500)
            .create();
        let url = server.host_with_port();

        let data_cache = Some(Arc::new(JsonDataCache::new(
            format!("http://{}/", url).parse().unwrap(),
            tokio::time::interval(Duration::from_secs(10))
        )));

        // Clear the task queue to simulate waiting for the value to be grabbed
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        let current_value = JsonDataCache::handle_cache_option(&data_cache).await;
        
        assert_eq!(current_value.is_none(), true);
    }

    #[tokio::test(start_paused=true)]
    async fn handles_invalid_json() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("BAD JSON")
            .create();
        let url = server.host_with_port();

        let data_cache = Some(Arc::new(JsonDataCache::new(
            format!("http://{}/", url).parse().unwrap(),
            tokio::time::interval(Duration::from_secs(10))
        )));

        // Clear the task queue to simulate waiting for the value to be grabbed
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        let current_value = JsonDataCache::handle_cache_option(&data_cache).await;
        
        assert_eq!(current_value.is_none(), true);
    }

    #[tokio::test]
    async fn handles_bad_fetch() {
        let data_cache = Some(Arc::new(JsonDataCache::new(
            "http://example.bad.domain/".parse().unwrap(),
            tokio::time::interval(Duration::from_secs(10))
        )));

        // Wait to clear request timeout for bad domain
        tokio::time::sleep(Duration::from_secs(7)).await;

        let current_value = JsonDataCache::handle_cache_option(&data_cache).await;
        
        assert_eq!(current_value.is_none(), true);
    }

    #[tokio::test(start_paused=true)]
    async fn none_option_yields_none() {
        let current_value = JsonDataCache::handle_cache_option(&None).await;
        
        current_value.expect("Should be Some")
            .as_null().expect("Should be null")
    }

}