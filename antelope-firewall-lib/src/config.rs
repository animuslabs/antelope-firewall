// Parses config and returns a firewall

use std::{collections::HashSet, sync::Arc};

use chrono::Duration;
use jsonpath::Selector;
use reqwest::Url;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{firewall_builder::{RoutingModeState, AntelopeFirewall}, filter::Filter, ratelimiter::{RateLimiter, IncrementMode}, healthcheck::HealthChecker};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub routing_mode: RoutingMode,
    pub port: u16,
    pub prometheus_port: Option<u16>,

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
    pub block_contracts: Vec<String>,
    pub block_ips: Vec<String>,
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
    #[serde(rename = "sender")]
    Sender,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RatelimitConfig {
    pub name: String,
    pub limit_on: RatelimitType,
    pub bucket_type: RatelimitBucket,

    pub limit: u64,
    pub window_duration: u64,
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
}

pub async fn from_config(config: Config) -> Result<AntelopeFirewall, ()> {
    let mut firewall = AntelopeFirewall::new(config.routing_mode.to_state());


    {
        let mut ip_guard = BLOCKED_IPS.write().await;
        let mut contract_guard = BLOCKED_CONTRACTS.write().await;
        if let Some(filter) = config.filter {
            for ip in filter.block_ips {
                ip_guard.insert(ip);
            }
            for contract in filter.block_contracts {
                contract_guard.insert(contract);
            }
        }
    }

    firewall = firewall.add_filter(Filter::new(
        "Filter".into(),
        Box::new(|(req, value, _)| Box::pin(async move {
            println!("{:?}", req);
            if !PUSH_ENDPOINTS.contains(&req.uri.to_string()) && !GET_ENDPOINTS.contains(&req.uri.to_string()) {
                return false;
            } else if BLOCKED_IPS.read().await.contains(&req.ip.to_string()) {
                return false;
            }
            true
        })),
        None
    ));

    for ratelimit in config.ratelimit {
        firewall = firewall.add_ratelimiter(RateLimiter::new(
            ratelimit.name,
            Box::new(|_| Box::pin(async { true })),
            match ratelimit.bucket_type {
                //TODO: finish contract and sender
                RatelimitBucket::Contract => Box::new(|(_, body, _)| Box::pin(async move { 
                    let selector = Selector::new("$.unpacked_trx.actions.*.account").unwrap();
                    selector.find(&body).into_iter().filter_map(|found| found.as_str().map(|account| account.to_string())).collect::<HashSet<String>>()
                 })),
                RatelimitBucket::IP => Box::new(|(req, _, _)| Box::pin(async move { 
                    HashSet::from([req.ip.to_string()]) 
                })),
                RatelimitBucket::Sender => Box::new(|(_, body, _)| Box::pin(async move {
                    let selector = Selector::new("$.unpacked_trx.actions.*.authorization.*.actor").unwrap();
                    selector.find(&body).into_iter().filter_map(|found| found.as_str().map(|actor| actor.to_string())).collect::<HashSet<String>>()
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
        push_nodes.insert((node.url.parse::<Url>().map_err(|_| ())?, node.weight.unwrap_or(1)));
    }

    let mut push_nodes_guard = PUSH_NODES.write().await;
    *push_nodes_guard = push_nodes.clone();
    drop(push_nodes_guard);

    let mut get_nodes: HashSet<(Url, u64)> = HashSet::new();
    for node in config.get_nodes {
        get_nodes.insert((node.url.parse::<Url>().map_err(|_| ())?, node.weight.unwrap_or(1)));
    }

    let mut get_nodes_guard = GET_NODES.write().await;
    *get_nodes_guard = get_nodes.clone();
    drop(get_nodes_guard);

    let nodes: HashSet<Url> = get_nodes.iter().chain(push_nodes.iter()).map(|(url, _)| url.clone()).collect();

    firewall = firewall.add_matching_rule(Box::new(move |(req, _, _, _)| Box::pin(async move {
        println!("Trying to match: {:?}", req);
        if GET_ENDPOINTS.contains(&req.uri.to_string()) {
            println!("Matched as get: {:?}", GET_NODES.read().await.clone());
            return GET_NODES.read().await.clone();
        } else if PUSH_ENDPOINTS.contains(&req.uri.to_string()) {
            println!("Matched as push");
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
            println!("Checking healthcheck: {:?}", nodes);
            if let Some(ref h) = *healthcheck_guard {
                h.filter_healthy_urls(nodes).await
            } else {
                nodes
            }
        })));
    }

    Ok(firewall)
}
