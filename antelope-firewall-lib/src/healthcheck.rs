use chrono::{Utc, Duration, DateTime, NaiveDateTime};
use reqwest::Url;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc
};
use tokio::sync::RwLock;

use log::{info, error as err};

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
            let parsed_datetime = DateTime::<Utc>::from_utc(
                NaiveDateTime::parse_from_str(&body.head_block_time, "%Y-%m-%dT%H:%M:%S.%f")
                    .map_err(|e| format!("error while parsing datetime: {}", e.to_string()))?,
                Utc
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
