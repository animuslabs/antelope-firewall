use chrono::Utc;
use reqwest::Url;
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;

struct HealthChecker {
    nodes: Vec<Url>,
    duration: Duration,
    healthy_map: RwLock<HashMap<Url, bool>>,
}

use crate::api_responses::GetInfoReponse;

impl HealthChecker {
    pub async fn start(nodes: Vec<Url>, duration: Duration) -> Arc<Self> {
        let checker = Arc::new(HealthChecker {
            nodes,
            duration,
            healthy_map: RwLock::new(HashMap::new()),
        });

        let checker_clone = Arc::clone(&checker);
        tokio::task::spawn(async move {
            let client = reqwest::Client::new();
            loop {
                for node_url in &checker_clone.nodes {
                    let node_res = client.post(format!("{}/get_info", node_url,)).send().await;

                    let mut get_info_response = checker_clone.healthy_map.write().await;
                    let healthy = categorize_response_healthy(node_res, 0.5).await;
                    get_info_response.insert(node_url.clone(), healthy);
                    drop(get_info_response);
                    println!("Healthcheck on node {} returned: {}", node_url, healthy);
                }
                tokio::time::sleep(checker_clone.duration).await;
            }
        });
        checker
    }

    pub async fn is_url_healthy(self: Arc<Self>, url: &Url) -> bool {
        let healthy_map = self.healthy_map.read().await;
        match healthy_map.get(url) {
            Some(healthy) => *healthy,
            None => false
        }
    }
}

async fn categorize_response_healthy(
    node_response: Result<reqwest::Response, reqwest::Error>,
    healthy_after: f32,
) -> bool {
    match node_response.and_then(|r| r.error_for_status()) {
        Ok(res) => {
            let body = res.json::<GetInfoReponse>().await;
            let healthy_after = Utc::now().timestamp() - healthy_after as i64;
            body.map(|b| b.head_block_time.timestamp() > healthy_after)
                .unwrap_or(false)
        }
        Err(_) => false,
    }
}
