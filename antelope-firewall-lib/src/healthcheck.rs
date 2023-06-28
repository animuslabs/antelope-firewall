use chrono::{Utc, Duration};
use reqwest::Url;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc
};
use tokio::sync::RwLock;

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
                    let node_res = client.post(format!("{}/v1/chain/get_info", node_url,)).send().await;

                    let mut get_info_response = checker_clone.healthy_map.write().await;
                    let healthy = categorize_response_healthy(node_res, grace_period).await;
                    get_info_response.insert(node_url.clone(), healthy);
                    drop(get_info_response);
                    println!("Healthcheck on node {} returned: {}", node_url, healthy);
                }
                tokio::time::sleep(checker_clone.duration.to_std().unwrap()).await;
            }
        });
        checker
    }

    pub async fn filter_healthy_urls(self: &Self, urls: HashSet<(Url, u64)>) -> HashSet<(Url, u64)> {
        let healthy_map = self.healthy_map.read().await;
        urls.into_iter().filter(|(url, weight)| *healthy_map.get(url).unwrap_or(&false)).collect()
    }
}

async fn categorize_response_healthy(
    node_response: Result<reqwest::Response, reqwest::Error>,
    healthy_after: chrono::Duration,
) -> bool {
    match node_response.and_then(|r| r.error_for_status()) {
        Ok(res) => {
            let body = res.json::<GetInfoReponse>().await;
            let healthy_after = Utc::now() - healthy_after;
            body.map(|b| b.head_block_time > healthy_after)
                .unwrap_or(false)
        }
        Err(_) => false,
    }
}
