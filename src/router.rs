use std::{collections::{HashMap, HashSet}, path::Path};
use thiserror::Error;

use lazy_static::lazy_static;
use serde::Deserialize;

use crate::healthcheck::HEALTHY_MAP;

lazy_static! {
    // TODO: Add hyperion
    pub static ref ENDPOINTS: HashSet<String> = [
        "/get_account", "/get_block", "/get_block_info", "/get_info", "/push_transaction",
        "/send_transaction", "/push_transactions", "/get_block_header_state", "/get_abi",
        "/get_currency_balance", "/get_currency_stats", "/get_required_keys", "/get_producers",
        "/get_raw_code_and_abi", "/get_scheduled_transaction", "/get_table_by_scope",
        "/get_table_rows", "/get_kv_table_rows", "/abi_json_to_bin", "/abi_bin_to_json",
        "/get_code", "/get_raw_abi", "/get_activated_protocol_features",
        "/get_accounts_by_authorizers"
    ].iter().map(|s| s.to_string()).collect();
    
    pub static ref PATHS_MAP: HashMap<String, HashSet<String>> = {
        let all: HashSet<String> = [
            "get_account", "get_block", "get_block_info", "get_info", "push_transaction",
            "send_transaction", "push_transactions", "get_block_header_state", "get_abi",
            "get_currency_balance", "get_currency_stats", "get_required_keys", "get_producers",
            "get_raw_code_and_abi", "get_scheduled_transaction", "get_table_by_scope",
            "get_table_rows", "get_kv_table_rows", "abi_json_to_bin", "abi_bin_to_json",
            "get_code", "get_raw_abi", "get_activated_protocol_features",
            "get_accounts_by_authorizers"
        ].iter().map(|s| s.to_string()).collect();

        let get: HashSet<String> = [
            "get_account", "get_block", "get_block_info", "get_info",
            "get_block_header_state", "get_abi", "get_currency_balance", 
            "get_currency_stats", "get_required_keys", "get_producers",
            "get_raw_code_and_abi", "get_scheduled_transaction", "get_table_by_scope",
            "get_table_rows", "get_kv_table_rows", "abi_json_to_bin", "abi_bin_to_json",
            "get_code", "get_raw_abi", "get_activated_protocol_features",
            "get_accounts_by_authorizers"
        ].iter().map(|s| s.to_string()).collect();
        
        let push: HashSet<String> = [
            "send_transaction", "push_transactions", "push_transaction"
        ].iter().map(|s| s.to_string()).collect();

        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        map.insert("all".into(), all.clone());
        all.into_iter().for_each(|path| {
            map.insert(path.clone(), HashSet::from([path]));
        });

        map.insert("get".into(), get);
        map.insert("push".into(), push);

        map
    };
}

#[derive(Debug, Clone)]
pub struct Router {
    pub nodes: Vec<(String, u64, HashSet<String>)>,
    pub routing_mode: RoutingMode,
    pub port: u16,
}

impl Router {
    /// Panics if a node has an invalid endpoint group. Check before calling
    pub fn from_config(config: &Config) -> Self {
        Router {
            routing_mode: config.routing_mode,
            nodes: config.nodes.iter().map(|n| (
                n.url.clone(), n.routing_weight.unwrap_or(1), n.can_handle.iter().fold(HashSet::new(), |mut acc, keyword| {
                    acc.extend(PATHS_MAP.get(keyword).unwrap().clone());
                    acc
                })
            )).collect(),
            port: config.port
        }
    }
    pub async fn get_nodes_for_path(&self, path: &str) -> Vec<(u64, String)> {
        let x = HEALTHY_MAP.lock().await;
        let mut matching_nodes = vec!();
        self.nodes.iter().for_each(|(node_url, routing_weight, accepts_paths)| {
            if accepts_paths.contains(&path[1..]) && *x.get(node_url).unwrap_or(&false) {
                matching_nodes.push((*routing_weight, node_url.clone()));
            }
        });
        matching_nodes
    }
}

#[derive(Error, Debug, Clone)]
pub enum ParseConfigError {
    #[error("Failed to parse config file, received error: `{0}`")]
    InvalidFormat(String),
    #[error("Couldn't read config file, received error: `{0}`")]
    CouldntReadFile(String),
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub nodes: Vec<Node>,
    pub routing_mode: RoutingMode,
    pub port: u16,
    pub healthcheck_interval: u64,
    pub healthcheck_time_to_invalid: u64,
    pub base_ip_ratelimit_config: Option<SlidingWindowParams>,
    pub failure_ratelimit_config: Option<SlidingWindowParams>,
}

pub fn parse_config_from_file(path: &Path) -> Result<(Router, Config), ParseConfigError> {
    let raw_config: Config = toml::from_str(
        &std::fs::read_to_string(path)
            .map_err(|e| ParseConfigError::CouldntReadFile(e.to_string()))?
    ).map_err(|e| ParseConfigError::InvalidFormat(e.to_string()))?;

    // Check to see if config says a node can handle an invalid endpoint group
    match raw_config.nodes.iter()
        .find_map(|node| node.can_handle.iter()
            .find_map(|endpoint| 
                if !PATHS_MAP.contains_key(endpoint) {
                    Some(ParseConfigError::InvalidFormat(
                        format!("{} is an invalid endpoint or endpoint group.", endpoint)
                    ))
                } else {
                    None
                }
            )
        ) {
        Some(e) => {
            Err(e)
        },
        None => {
            Ok((Router::from_config(&raw_config), raw_config))
        },
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct SlidingWindowParams {
    pub secs_in_window: u64,
    pub allowed_per_window: u64,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    #[serde(rename = "round_robin")]
    RoundRobin,
    #[serde(rename = "least_connections")]
    LeastConnections,
    #[serde(rename = "random")]
    Random,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Node {
    name: String,
    url: String,
    can_handle: Vec<String>,
    routing_weight: Option<u64>,
}
