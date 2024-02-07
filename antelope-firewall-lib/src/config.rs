// Parses config and returns a firewall

use std::{collections::{HashSet, HashMap}, sync::Arc, net::SocketAddr};

use chrono::Duration;
use jsonpath::Selector;
use reqwest::Url;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{firewall_builder::{RoutingModeState, AntelopeFirewall}, filter::Filter, ratelimiter::{RateLimiter, IncrementMode}, healthcheck::HealthChecker, prometheus::start_prometheus_exporter};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub routing_mode: RoutingMode,
    pub address: String,
    pub prometheus_address: Option<String>,

    pub healthcheck: Option<HealthcheckConfig>,

    pub filter: Option<FilterConfig>,
    pub ratelimit: Vec<RatelimitConfig>,

    pub push_nodes: Vec<Node>,
    pub get_nodes: Vec<Node>,
}

#[derive(Deserialize, Debug)]
pub struct Node {
    pub name: String,
    pub url: String,
    pub weight: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct HealthcheckConfig {
    pub interval: u64,
    pub grace_period: u64,
}

#[derive(Deserialize, Debug)]
pub struct FilterConfig {
    pub block_contracts: Option<Vec<String>>,
    pub block_ips: Option<Vec<String>>,
    pub allow_only_contracts: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatelimitType {
    #[serde(rename = "attempt")]
    Attempt,
    #[serde(rename = "failure")]
    Failure,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatelimitBucket {
    #[serde(rename = "contract")]
    Contract,
    #[serde(rename = "ip")]
    IP,
    #[serde(rename = "authorizer")]
    Sender,
    #[serde(rename = "table")]
    Table,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RatelimitConfig {
    pub name: String,
    pub limit_on: RatelimitType,
    pub bucket_type: RatelimitBucket,

    pub limit: u64,
    pub window_duration: u64,
    pub select_accounts: Option<Vec<String>>,
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

impl RoutingMode {
    pub fn to_state(&self) -> RoutingModeState {
        match self {
            RoutingMode::RoundRobin => RoutingModeState::base_round_robin(),
            RoutingMode::LeastConnections => RoutingModeState::base_least_connected(),
            RoutingMode::Random => RoutingModeState::base_random(),
        }
    }
}

lazy_static::lazy_static! {
    pub static ref BLOCKED_IPS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());
    pub static ref BLOCKED_CONTRACTS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());
    pub static ref ALLOW_ONLY_CONTRACTS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());

    pub static ref PUSH_ENDPOINTS: HashSet<String> = HashSet::from([
        "/v1/chain/push_transaction".into(),
        "/v1/chain/send_transaction".into(),
        "/v1/chain/push_transactions".into(),
        "/v1/chain/send_transaction2".into(),
        "/v1/chain/compute_transaction".into(),
        "/v1/chain/send_read_only_transaction".into(),
        "/v1/chain/push_block".into(),
    ]);
    pub static ref GET_ENDPOINTS: HashSet<String> = HashSet::from([
        "/v1/chain/get_account".into(),
        "/v1/chain/get_block".into(),
        "/v1/chain/get_block_info".into(),
        "/v1/chain/get_info".into(),
        "/v1/chain/get_block_header_state".into(),
        "/v1/chain/get_abi".into(),
        "/v1/chain/get_currency_balance".into(),
        "/v1/chain/get_currency_stats".into(),
        "/v1/chain/get_required_keys".into(),
        "/v1/chain/get_producers".into(),
        "/v1/chain/get_raw_code_and_abi".into(),
        "/v1/chain/get_scheduled_transactions".into(),
        "/v1/chain/get_table_by_scope".into(),
        "/v1/chain/get_table_rows".into(),
        "/v1/chain/get_code".into(),
        "/v1/chain/get_raw_abi".into(),
        "/v1/chain/get_activated_protocol_features".into(),
        "/v1/chain/get_accounts_by_authorizers".into(),
        "/v1/chain/get_transaction_status".into(),
        "/v1/chain/get_producer_schedule".into()
    ]);

    pub static ref PUSH_NODES: RwLock<HashSet<(Url, u64)>> = RwLock::new(HashSet::new());
    pub static ref GET_NODES: RwLock<HashSet<(Url, u64)>> = RwLock::new(HashSet::new());

    pub static ref HEALTH_CHECKER: RwLock<Option<Arc<HealthChecker>>> = RwLock::new(None);

    pub static ref SELECT_ACCOUNTS: RwLock<HashMap<String, HashSet<String>>> = RwLock::new(HashMap::new());
}

pub async fn from_config(config: Config) -> Result<AntelopeFirewall, String> {
    // TODO: Add proper error handling
    if let Some(socket_str) = config.prometheus_address {
        let prometheus_address: SocketAddr = socket_str.parse().unwrap();
        start_prometheus_exporter(prometheus_address);
    }

    let socket_addr: SocketAddr = config.address.parse().unwrap();
    let mut firewall = AntelopeFirewall::new(
        config.routing_mode.to_state(),
        socket_addr
    );

    if let Some(filter_config) = config.filter {
        if (filter_config.block_contracts.clone().map_or(0, |v| v.len()) > 0) &&
            (filter_config.allow_only_contracts.clone().map_or(0, |v| v.len()) > 0) {
            return Err("Cannot block and allow contracts at the same time.".into());
        }

        let mut ip_guard = BLOCKED_IPS.write().await;
        let mut contract_guard = BLOCKED_CONTRACTS.write().await;
        let mut allow_contract_guard = ALLOW_ONLY_CONTRACTS.write().await;
        if let Some(ips) = filter_config.block_ips {
            for ip in ips {
                ip_guard.insert(ip);
            }
        }
        if let Some(contracts) = filter_config.block_contracts {
            for contract in contracts {
                contract_guard.insert(contract);
            }
        }
        if let Some(contracts) = filter_config.allow_only_contracts {
            for contract in contracts {
                allow_contract_guard.insert(contract);
            }
        }
    }

    firewall = firewall.add_filter(Filter::new(
        "Filter".into(),
        Box::new(|(req, body, _)| Box::pin(async move {
            if !PUSH_ENDPOINTS.contains(&req.uri.to_string()) && !GET_ENDPOINTS.contains(&req.uri.to_string()) {
                return false;
            } else if BLOCKED_IPS.read().await.contains(&req.ip.to_string()) {
                return false;
            } else {
                let selector = Selector::new("$.unpacked_trx.actions.*.account").unwrap();
                let contract_guard = BLOCKED_CONTRACTS.read().await;
                let allow_contract_guard = ALLOW_ONLY_CONTRACTS.read().await;
                if contract_guard.is_empty() && allow_contract_guard.is_empty() {
                    return true
                } else if contract_guard.is_empty() {
                    return selector.find(&body).into_iter()
                        .filter_map(|found| found.as_str().map(|account| account.to_string()))
                        .all(|account| {
                            allow_contract_guard.contains(&account)
                        });
                } else {
                    return !selector.find(&body).into_iter()
                        .filter_map(|found| found.as_str().map(|account| account.to_string()))
                        .any(|account| {
                            contract_guard.contains(&account)
                        });
                }
            }
        })),
        None
    ));
    
    let mut names = HashSet::new();
    for ratelimit in config.ratelimit {
        if names.contains(&ratelimit.name) {
            return Err(format!("Duplicate name for ratelimiter '{}'. Names must be unique.", ratelimit.name));
        } else {
            names.insert(ratelimit.name.clone());
        }

        {
            let mut select_accounts_guard = SELECT_ACCOUNTS.write().await;
            if let Some(select_accounts) = ratelimit.select_accounts {
                select_accounts_guard.insert(ratelimit.name.clone(), HashSet::from_iter(select_accounts));
            }
        }

        let select_accounts_guard = SELECT_ACCOUNTS.write().await;
        firewall = firewall.add_ratelimiter(RateLimiter::new(
            ratelimit.name,
            Box::new(|_| Box::pin(async { true })),
            match ratelimit.bucket_type {
                RatelimitBucket::Contract => Box::new(|(name, _, body, _)| Box::pin(async move { 
                    let selector = Selector::new("$.unpacked_trx.actions.*.account").unwrap();
                    let unfiltered = selector.find(&body).into_iter().filter_map(|found| found.as_str().map(|account| account.to_string())).collect::<HashSet<String>>();
                    let select_accounts_map = SELECT_ACCOUNTS.read().await;
                    if let Some(select_accounts) = select_accounts_map.get(name.as_ref()) {
                        unfiltered.into_iter().filter(|account| {
                            select_accounts.contains(account)
                        }).collect::<HashSet<String>>()
                    } else {
                        unfiltered
                    }
                })),
                RatelimitBucket::IP => Box::new(|(_, req, _, _)| Box::pin(async move { 
                    HashSet::from([req.ip.to_string()]) 
                })),
                RatelimitBucket::Sender => Box::new(|(name, _, body, _)| Box::pin(async move {
                    let selector = Selector::new("$.unpacked_trx.actions.*.authorization.*.actor").unwrap();
                    let unfiltered = selector.find(&body).into_iter().filter_map(|found| found.as_str().map(|actor| actor.to_string())).collect::<HashSet<String>>();
                    let select_accounts_map = SELECT_ACCOUNTS.read().await;
                    if let Some(select_accounts) = select_accounts_map.get(name.as_ref()) {
                        unfiltered.into_iter().filter(|account| {
                            select_accounts.contains(account)
                        }).collect::<HashSet<String>>()
                    } else {
                        unfiltered
                    }
                })),
                RatelimitBucket::Table => Box::new(|(name, req, body, _)| Box::pin(async move {
                    let unfiltered = match (
                        req.uri == "/v1/chain/get_table_rows" || req.uri == "/v1/chain/get_table_by_scope",
                        body.get("code")
                    ) {
                        (true, Some(serde_json::Value::String(code))) => HashSet::from([code.clone()]),
                        _ => HashSet::new()
                    };
                    let select_accounts_map = SELECT_ACCOUNTS.read().await;
                    if let Some(select_accounts) = select_accounts_map.get(name.as_ref()) {
                        unfiltered.into_iter().filter(|account| {
                            select_accounts.contains(account)
                        }).collect::<HashSet<String>>()
                    } else {
                        unfiltered
                    }
                })),
            },
            Box::new(move |_| Box::pin(async move { ratelimit.limit })),
            match ratelimit.limit_on {
                RatelimitType::Attempt => IncrementMode::Before(Box::new(|_| Box::pin(async move { 1 }))),
                RatelimitType::Failure => IncrementMode::After(Box::new(|(_, _, res, _)| Box::pin(async move { if res.1.is_success() { 0 } else { 1 } }))),
            },
            None,
            ratelimit.window_duration,
        ));
    }

    let mut push_nodes: HashSet<(Url, u64)> = HashSet::new();
    for node in config.push_nodes {
        let weight = node.weight.unwrap_or(1);
        if weight == 0 {
            return Err(format!("Weight for node '{}' must be greater than 0.", node.name));
        }
        push_nodes.insert((
            node.url.parse::<Url>().map_err(|_| format!("Could not parse node '{}' as url.", node.url))?,
            weight
        ));
    }

    let mut push_nodes_guard = PUSH_NODES.write().await;
    *push_nodes_guard = push_nodes.clone();
    drop(push_nodes_guard);

    let mut get_nodes: HashSet<(Url, u64)> = HashSet::new();
    for node in config.get_nodes {
        let weight = node.weight.unwrap_or(1);
        if weight == 0 {
            return Err(format!("Weight for node '{}' must be greater than 0.", node.name));
        }
        get_nodes.insert((
            node.url.parse::<Url>().map_err(|_| format!("Could not parse node '{}' as url.", node.url))?,
            weight
        ));
    }

    let mut get_nodes_guard = GET_NODES.write().await;
    *get_nodes_guard = get_nodes.clone();
    drop(get_nodes_guard);

    let nodes: HashSet<Url> = get_nodes.iter().chain(push_nodes.iter()).map(|(url, _)| url.clone()).collect();

    firewall = firewall.add_matching_rule(Box::new(move |(req, _, _, _)| Box::pin(async move {
        if GET_ENDPOINTS.contains(&req.uri.to_string()) {
            return GET_NODES.read().await.clone();
        } else if PUSH_ENDPOINTS.contains(&req.uri.to_string()) {
            return PUSH_NODES.read().await.clone();
        }
        HashSet::new()
    })));

    if let Some(healthcheck) = config.healthcheck {
        {
            let mut healthcheck_guard = HEALTH_CHECKER.write().await;
            *healthcheck_guard = Some(HealthChecker::start(
                nodes.into_iter().collect(),
                Duration::seconds(healthcheck.interval as i64),
                Duration::seconds(healthcheck.grace_period as i64)
            ).await);
        }
        firewall = firewall.add_matching_rule(Box::new(move |(_, _, _, nodes)| Box::pin(async move {
            let healthcheck_guard = HEALTH_CHECKER.read().await;
            if let Some(ref h) = *healthcheck_guard {
                h.filter_healthy_urls(nodes).await
            } else {
                nodes
            }
        })));
    }

    Ok(firewall)
}

#[cfg(test)]
mod tests {
    use core::time;
    use std::thread;

    use chrono::Utc;
    use hyper::StatusCode;
    use reqwest::Client;
    use serde_json::from_str;

    use super::*;
    
    #[tokio::test]
    #[serial_test::serial]
    async fn parses_body() {
        let _ = env_logger::builder().is_test(true).try_init();

        let default_config = include_str!("../test-configs/basic.toml");
        let config = toml::from_str::<Config>(default_config).expect("Default config contains an error");
        let firewall = from_config(config).await.expect("Default config unable to build");

        tokio::spawn(async move {
            let err = firewall.build().run().await;
            panic!("Error!: {:?}", err);
        });
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let client = Client::new();
        let result = client.post("http://127.0.0.1:3000/v1/chain/get_block_info")
            .body("{\"block_num\":100}")
            .send()
            .await;
        let response = result.expect("Encountered error getting info");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.expect("Error while getting bytes");
        let json_body = from_str::<serde_json::Value>(&body).expect("response not json");
        if let Some(id) = json_body.as_object().and_then(|map| map.get("id").and_then(|o| o.as_str())) {
            assert_eq!(id, "0000006492871283c47f6ef57b00cf534628eb818c34deb87ea68a3557254c6b");
        } else {
            panic!("invalid body")
        }
    }
    
}