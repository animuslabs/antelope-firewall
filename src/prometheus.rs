use std::net::{Ipv4Addr, SocketAddr};

use lazy_static::lazy_static;
use prometheus_exporter::prometheus::{register_counter, core::{GenericCounter, AtomicF64}};

lazy_static! {
    pub static ref REQUESTS_RECEIVED: GenericCounter<AtomicF64> = register_counter!("reqs_received", "Total Requests Received").unwrap();
    pub static ref REQUESTS_RATELIMITED_GENERAL: GenericCounter<AtomicF64> = register_counter!("reqs_ratelimited_general", "Requests Ratelimited by General Ratelimiter").unwrap();
    pub static ref REQUESTS_RATELIMITED_FAILURE: GenericCounter<AtomicF64> = register_counter!("reqs_healthy", "Requests Ratelimited by Failure Ratelimiter").unwrap();
    pub static ref REQUESTS_FAILED_TO_ROUTE: GenericCounter<AtomicF64> = register_counter!("reqs_failed_to_route", "Requests Unable to be Routed Successfully").unwrap();
    pub static ref ERROR_NODE_RESPONSES: GenericCounter<AtomicF64> = register_counter!("node_responses_error", "Node responses that had errors").unwrap();
    pub static ref SUCCESS_NODE_RESPONSES: GenericCounter<AtomicF64> = register_counter!("node_responses_success", "Node responses that returned success").unwrap();
}

pub fn start_prometheus_exporter(port: u16) -> Result<(), String> {
    let mut builder = prometheus_exporter::Builder::new(SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    builder
        .with_endpoint("metrics")
        .map_err(|e| e.to_string())?;
    builder.start()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
