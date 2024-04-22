use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use reqwest::Url;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc
};
use tokio::sync::RwLock;

use log::{info, error as err};

#[derive(Debug)]
pub struct HealthChecker {
    nodes: Vec<Url>,
    duration: Duration,
    grace_period: Duration,
    healthy_map: RwLock<HashMap<Url, bool>>,
}

use crate::api_responses::GetInfoReponse;

impl HealthChecker {
    pub async fn start(nodes: Vec<Url>, duration: Duration, grace_period: Duration) -> Arc<Self> {
        let checker = Arc::new(HealthChecker {
            nodes,
            duration,
            grace_period,
            healthy_map: RwLock::new(HashMap::new()),
        });

        {
            let mut healthy_map_guard = checker.healthy_map.write().await;
            for node_url in &checker.nodes {
                healthy_map_guard.insert(node_url.clone(), true);
            }
        }

        let checker_clone = Arc::clone(&checker);
        tokio::task::spawn(async move {
            let client = reqwest::Client::new();
            loop {
                for node_url in &checker_clone.nodes {
                    let get_info_url = format!("{}v1/chain/get_info", node_url);
                    let node_res = client.post(get_info_url.clone()).send().await;

                    let mut get_info_response = checker_clone.healthy_map.write().await;
                    let healthy = categorize_response_healthy(node_res, grace_period).await;
                    match healthy {
                        Ok(_) => {
                            get_info_response.insert(node_url.clone(), true);
                            drop(get_info_response);
                            info!("Healthcheck on node at {} succeeded", get_info_url);
                        },
                        Err(e) => {
                            get_info_response.insert(node_url.clone(), false);
                            drop(get_info_response);
                            err!("Healthcheck on node at {} failed with error: {}", get_info_url, e);
                        },
                    }
                }
                tokio::time::sleep(checker_clone.duration.to_std().unwrap()).await;
            }
        });
        checker
    }

    pub async fn filter_healthy_urls(self: &Self, urls: HashSet<(Url, u64)>) -> HashSet<(Url, u64)> {
        let healthy_map = self.healthy_map.read().await;
        urls.into_iter().filter(|(url, _)| *healthy_map.get(url).unwrap_or(&false)).collect()
    }
}

async fn categorize_response_healthy(
    node_response: Result<reqwest::Response, reqwest::Error>,
    healthy_after: chrono::Duration,
) -> Result<(), String> {
    match node_response.and_then(|r| r.error_for_status()) {
        Ok(res) => {
            let body = res.json::<GetInfoReponse>().await
                .map_err(|e| e.to_string())?;
            let healthy_after = Utc::now() - healthy_after;
            let parsed_datetime = Utc.from_utc_datetime(
                &NaiveDateTime::parse_from_str(&body.head_block_time, "%Y-%m-%dT%H:%M:%S.%f")
                    .map_err(|e| format!("error while parsing datetime: {}", e.to_string()))?
            );
            if parsed_datetime >= healthy_after  {
                Ok(())
            } else {
                Err(format!("Parsed head_block_time of {}, which was older than healthy time of {}", parsed_datetime, healthy_after))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[tokio::test(start_paused=true)]
    async fn parses_response() {
        let current_time = chrono::Utc::now();
        
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/v1/chain/get_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                "{{\"head_block_time\":\"{}\"}}",
                current_time.format("%Y-%m-%dT%H:%M:%S.%f")
            ))
            .create();
        let url = Url::parse(format!("http://{}/", server.host_with_port()).as_str()).expect("Invalid url");
        
        let healthchecker = HealthChecker::start(
            Vec::from([url.clone()]),
            Duration::seconds(5),
            Duration::seconds(1),
        ).await;

        for _ in 0..1000 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        let result = healthchecker.filter_healthy_urls(HashSet::from([(url, 4)])).await;
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn removes_before_grace_period() {
        let old_time = chrono::Utc::now().checked_sub_signed(Duration::seconds(3)).expect("Invalid time");
        
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/v1/chain/get_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                "{{\"head_block_time\":\"{}\"}}",
                old_time.format("%Y-%m-%dT%H:%M:%S.%f")
            ))
            .create();
        
        let url = Url::parse(format!("http://{}/", server.host_with_port()).as_str()).expect("Invalid url");

        let healthchecker = HealthChecker::start(
            Vec::from([url.clone()]),
            Duration::seconds(5),
            Duration::seconds(1),
        ).await;

        for _ in 0..1000 {
            let result = healthchecker.filter_healthy_urls(HashSet::from([(url.clone(), 4)])).await;
            if result.len() == 0 {
                return;
            } else if result.len() == 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            } else {
                panic!("Unexpected number of results");
            }
        }
        panic!("Never removed item");
    }

    #[tokio::test]
    async fn removes_invalid_json() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/v1/chain/get_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("abc")
            .create();
        
        let url = Url::parse(format!("http://{}/", server.host_with_port()).as_str()).expect("Invalid url");

        let healthchecker = HealthChecker::start(
            Vec::from([url.clone()]),
            Duration::seconds(5),
            Duration::seconds(1),
        ).await;

        for _ in 0..1000 {
            let result = healthchecker.filter_healthy_urls(HashSet::from([(url.clone(), 4)])).await;
            if result.len() == 0 {
                return;
            } else if result.len() == 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            } else {
                panic!("Unexpected number of results");
            }
        }
        panic!("Never removed item");
    }

    #[tokio::test]
    async fn removes_bad_format() {
        let old_time = chrono::Utc::now().checked_sub_signed(Duration::milliseconds(500)).expect("Invalid time");
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/v1/chain/get_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                "{{\"head_block_time\":\"{}\"}}",
                old_time.format("%Y-%m-%dBADFORMAT%H:%M:%S.%f")
            ))
            .create();
        
        let url = Url::parse(format!("http://{}/", server.host_with_port()).as_str()).expect("Invalid url");

        let healthchecker = HealthChecker::start(
            Vec::from([url.clone()]),
            Duration::seconds(5),
            Duration::seconds(1),
        ).await;

        for _ in 0..1000 {
            let result = healthchecker.filter_healthy_urls(HashSet::from([(url.clone(), 4)])).await;
            if result.len() == 0 {
                return;
            } else if result.len() == 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            } else {
                panic!("Unexpected number of results");
            }
        }
        panic!("Never removed item");
    }

    #[tokio::test]
    async fn only_removes_invalid() {
        let old_time_one = chrono::Utc::now().checked_sub_signed(Duration::milliseconds(0)).expect("Invalid time");
        let old_time_two = chrono::Utc::now().checked_sub_signed(Duration::milliseconds(5000)).expect("Invalid time");

        let mut server_one = mockito::Server::new_async().await;
        server_one.mock("POST", "/v1/chain/get_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                "{{\"head_block_time\":\"{}\"}}",
                old_time_one.format("%Y-%m-%dT%H:%M:%S.%f")
            ))
            .create();
        let mut server_two = mockito::Server::new_async().await;
        server_two.mock("POST", "/v1/chain/get_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                "{{\"head_block_time\":\"{}\"}}",
                old_time_two.format("%Y-%m-%dT%H:%M:%S.%f")
            ))
            .create();
        
        let url_one = Url::parse(format!("http://{}/", server_one.host_with_port()).as_str()).expect("Invalid url");
        let url_two = Url::parse(format!("http://{}/", server_two.host_with_port()).as_str()).expect("Invalid url");

        let healthchecker = HealthChecker::start(
            Vec::from([url_one.clone(), url_two.clone()]),
            Duration::seconds(5),
            Duration::seconds(1),
        ).await;

        for _ in 0..1000 {
            let result = healthchecker.filter_healthy_urls(HashSet::from([(url_one.clone(), 4), (url_two.clone(), 1)])).await;
            if result.len() == 1 {
                return;
            } else if result.len() == 2 {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            } else {
                println!("{:?}", healthchecker);
                panic!("Unexpected number of results");
            }
        }
        panic!("Never removed item");
    }
}
