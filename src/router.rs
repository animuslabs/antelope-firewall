use std::collections::{HashMap, HashSet};

use lazy_static::lazy_static;
use serde::Deserialize;

lazy_static! {
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

#[derive(Debug)]
pub struct Router {
    pub nodes: Vec<(String, HashSet<String>)>,
}

impl Router {
    pub fn new() -> Self {
        let config: Config = toml::from_str(&std::fs::read_to_string("test/example.toml").unwrap()).unwrap();
        Router {
            nodes: config.nodes.into_iter().map(|n| (
                n.url.clone(), n.can_handle.into_iter().fold(HashSet::new(), |mut acc, keyword| {
                    acc.extend(PATHS_MAP.get(&keyword).unwrap().clone());
                    acc
                })
            )).collect(),
        }
    }
    pub fn get_nodes_for_path(&self, path: &str) -> Vec<String> {
        let mut matching_nodes = vec!();
        self.nodes.iter().for_each(|(node_url, accepts_paths)| {
            if accepts_paths.contains(&path[1..]) {
                matching_nodes.push(node_url.clone());
            }
        });
        matching_nodes
    }
}

#[derive(Deserialize)]
pub struct Config {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
pub struct Node {
    name: String,
    url: String,
    can_handle: Vec<String>,
}
