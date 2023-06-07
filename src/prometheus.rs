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

pub fn start_prometheus_exporter() {
    let mut builder = prometheus_exporter::Builder::new("127.0.0.1:9184".parse().unwrap());
    builder
        .with_endpoint("metrics")
        .unwrap();
    builder.start().unwrap();
}
