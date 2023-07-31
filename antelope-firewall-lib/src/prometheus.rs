use std::net::SocketAddr;

use lazy_static::lazy_static;
use prometheus_exporter::prometheus::{register_counter, core::{GenericCounter, AtomicF64}};

lazy_static! {
    pub static ref REQUESTS_RECEIVED: GenericCounter<AtomicF64> = register_counter!("reqs_received", "Total Requests Received").unwrap();
    pub static ref REQUESTS_FAILED_TO_ROUTE: GenericCounter<AtomicF64> = register_counter!("reqs_failed_to_route", "Requests Unable to be Routed Successfully").unwrap();
    pub static ref CLIENT_ERROR_NODE_RESPONSES: GenericCounter<AtomicF64> = register_counter!("node_responses_error", "Total node responses that had 4** errors").unwrap();
    pub static ref SERVER_ERROR_NODE_RESPONSES: GenericCounter<AtomicF64> = register_counter!("node_responses_error", "Total node responses that had 5** errors").unwrap();
    pub static ref SUCCESS_NODE_RESPONSES: GenericCounter<AtomicF64> = register_counter!("node_responses_success", "Total node responses that returned success").unwrap();
}

pub fn start_prometheus_exporter(socket_addr: SocketAddr) -> Result<(), String> {
    let mut builder = prometheus_exporter::Builder::new(socket_addr);
    builder
        .with_endpoint("metrics")
        .map_err(|e| e.to_string())?;
    builder.start()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
