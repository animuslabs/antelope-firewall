use crate::{api_responses::GetInfoReponse, CONFIG};
use std::{collections::HashMap, time::Duration};
use tokio::sync::Mutex;
use chrono::Utc;

lazy_static::lazy_static! {
    pub static ref HEALTHY_MAP: Mutex<HashMap<String, bool>> = Mutex::new(HashMap::new());
}

pub async fn start_healthcheck(nodes: Vec<String>) -> bool {
    let client = reqwest::Client::new();
    loop {
        for node_url in &nodes {
            let node_res = client.post(format!(
                "{}/get_info", node_url,
            ))
                .send()
                .await;
            
            let mut get_info_response = HEALTHY_MAP.lock().await;
            let healthy = determine_healthy(node_res).await;
            get_info_response.insert(node_url.clone(), healthy);
            drop(get_info_response);
            println!("Healthcheck on node {} returned: {}", node_url, healthy);
        }
        tokio::time::sleep(Duration::from_secs(CONFIG.healthcheck_interval)).await;
    }
}

pub async fn determine_healthy(node_response: Result<reqwest::Response, reqwest::Error>) -> bool {
    match node_response.and_then(|r| r.error_for_status()) {
        Ok(res) => {
            let body = res.json::<GetInfoReponse>().await;
            let healthy_after = Utc::now().timestamp() - CONFIG.healthcheck_time_to_invalid as i64;
            body.map(|b| b.head_block_time.timestamp() > healthy_after).unwrap_or(false)
        },
        Err(_) => false
    }
}
