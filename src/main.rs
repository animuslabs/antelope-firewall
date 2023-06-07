use std::cmp::Ordering;
use std::collections::HashMap;
use std::net::{SocketAddr, IpAddr};
use std::path::Path;


use clap::Parser;
use command_line::Args;
use healthcheck::start_healthcheck;
use http_body_util::{Full, BodyExt};
use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, Method};
use prometheus::start_prometheus_exporter;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use ratelimit::SlidingWindow;
use tokio::net::TcpListener;

mod router;
mod ratelimit;
mod healthcheck;
mod api_responses;
mod prometheus;
mod command_line;

use router::*;
use tokio::sync::Mutex;

use itertools::Itertools;
use itertools::FoldWhile::{Continue, Done};

use crate::prometheus::{REQUESTS_RECEIVED, REQUESTS_RATELIMITED_GENERAL, REQUESTS_RATELIMITED_FAILURE, ERROR_NODE_RESPONSES, SUCCESS_NODE_RESPONSES, REQUESTS_FAILED_TO_ROUTE};

lazy_static::lazy_static! {
    static ref ARGS: Args = Args::parse();
    static ref _CONFIG_TUPL: (Router, Config) = parse_config_from_file(Path::new(&ARGS.config)).unwrap();
    static ref CONFIG: Config = _CONFIG_TUPL.1.clone();
    static ref ROUTER: Router = _CONFIG_TUPL.0.clone();

    static ref FAILURE_RATELIMITER: Option<Mutex<SlidingWindow>> = CONFIG.failure_ratelimit_config
        .as_ref()
        .map(|params| Mutex::new(SlidingWindow::new(params.secs_in_window, params.allowed_per_window)));

    static ref IP_RATELIMITER: Option<Mutex<SlidingWindow>> = CONFIG.base_ip_ratelimit_config
        .as_ref()
        .map(|params| Mutex::new(SlidingWindow::new(params.secs_in_window, params.allowed_per_window)));

    static ref RR_COUNTS: HashMap<String, Mutex<u64>> = {
        let mut map = HashMap::new();
        ENDPOINTS.iter().for_each(|endpoint| {
            map.insert(endpoint.clone(), Mutex::new(0));
        });
        map
    };
    
    static ref LC_COUNTS: Mutex<HashMap<String, u64>> = Mutex::new(HashMap::new());
}

fn get_ratelimit_response(secs_in_window: u64) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(429)
        .header("Retry-After", secs_in_window)
        .header("Content-Type", "text/plain")
        .body(full("")).expect("Invalid Ratelimit Response")
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn handle_request(req: Request<hyper::body::Incoming>, ip: IpAddr) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    match req.method() {
        &Method::POST => {
            let (parts, body) = req.into_parts();
            println!("Handling request: {}", parts.uri.path());
            REQUESTS_RECEIVED.inc();

            // Handle Ratelimiters
            if let Some(limiter_mutex) = IP_RATELIMITER.as_ref() {
                let mut limiter = limiter_mutex.lock().await;
                if !limiter.check_and_increment(ip) {
                    REQUESTS_RATELIMITED_GENERAL.inc();
                    return Ok(get_ratelimit_response(limiter.window_duration));
                };
            }

            if let Some(limiter_mutex) = FAILURE_RATELIMITER.as_ref() {
                let mut limiter = limiter_mutex.lock().await;
                if !limiter.check_ip_passes(&ip) {
                    REQUESTS_RATELIMITED_FAILURE.inc();
                    return Ok(get_ratelimit_response(limiter.window_duration));
                };
            }

            let possible_nodes: Vec<(u64, String)> = ROUTER.get_nodes_for_path(parts.uri.path()).await;
            if possible_nodes.len() == 0 {
                println!("No nodes available for {}", parts.uri.path());
                REQUESTS_FAILED_TO_ROUTE.inc();
                return Ok(Response::builder()
                    .status(500)
                    .body(full("No nodes available for this path")).unwrap());
            }

            let matched_node_url = match ROUTER.routing_mode {
                RoutingMode::RoundRobin => {
                    let mut count = RR_COUNTS.get(parts.uri.path()).unwrap().lock().await;
                    let local_count = *count;
                    *count += 1;
                    drop(count);

                    let modulated = local_count % possible_nodes.iter().map(|(weight, _)| weight).sum::<u64>();

                    possible_nodes.iter().fold_while((modulated, String::new()), |(weights_left, s), (weight, url)| {
                        if weights_left < *weight {
                            Done((0, url.clone()))
                        } else {
                            Continue((weights_left.checked_sub(*weight).unwrap_or(0), s))
                        }
                    }).into_inner().1
                },
                RoutingMode::Random => {
                    let (weights, urls): (Vec<u64>, Vec<String>) = possible_nodes.into_iter().unzip();
                    let dist = WeightedIndex::new(weights).unwrap();
                    urls[dist.sample(&mut rand::thread_rng())].clone()
                },
                RoutingMode::LeastConnections => {
                    let mut lc_counts = LC_COUNTS.lock().await;
                    possible_nodes.into_iter()
                        .map(|(weight, url)| (*lc_counts.entry(url.clone()).or_insert(0) as f32 / weight as f32, url))
                        .min_by(|(weight1, _), (weight2, _)| weight1.partial_cmp(weight2).unwrap_or(Ordering::Equal))
                        .expect("There were no possible nodes even though it was checked earlier")
                        .1
                }
            };

            let client = reqwest::Client::new();

            let body_bytes = body.collect().await?.to_bytes();

            if ROUTER.routing_mode == RoutingMode::LeastConnections {
                let mut lc_counts = LC_COUNTS.lock().await;
                lc_counts.entry(matched_node_url.clone())
                    .and_modify(|e| *e += 1)
                    .or_insert(1);
            }

            println!("Sending request to node: {}", matched_node_url);
            // TODO: Add header that includes the src ip so nodes know who
            // made the request
            let node_res = client.post(format!(
                "{}/{}", matched_node_url, parts.uri.path(),
            ))
                .headers(parts.headers.clone())
                .body(body_bytes)
                .send()
                .await.unwrap();

            if ROUTER.routing_mode == RoutingMode::LeastConnections {
                let mut lc_counts = LC_COUNTS.lock().await;
                lc_counts.entry(matched_node_url.clone())
                    .and_modify(|e| *e = e.checked_sub(1).unwrap_or(0))
                    .or_insert(0);
            }

            // Update failure rate limiter.
            if let Some(limiter_mutex) = FAILURE_RATELIMITER.as_ref() {
                if node_res.status().is_client_error() || node_res.status().is_server_error() {
                    ERROR_NODE_RESPONSES.inc();
                    let mut limiter = limiter_mutex.lock().await;
                    limiter.increment_count(ip);
                }
            }

            let mut client_res = Response::builder()
                .status(node_res.status());

            client_res.headers_mut().map(|h| h.clone_from(node_res.headers()));
            let final_response = client_res.body(full(node_res.bytes().await.unwrap())).unwrap();
            SUCCESS_NODE_RESPONSES.inc();
            Ok(final_response)
        },
        _ => {
            Ok(Response::new(full("Invalid Method")))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let addr = SocketAddr::from(([127, 0, 0, 1], ROUTER.port));

    tokio::task::spawn(start_healthcheck(ROUTER.nodes.iter().map(|(url, _, _)| url.clone()).collect()));

    if let Some(port) = CONFIG.prometheus_port {
        start_prometheus_exporter(port)?;
    }

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            let address = stream.peer_addr()
                .map(|addr| addr.ip())
                .unwrap_or(IpAddr::from([127, 0, 0, 1]));
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(stream, service_fn(|r| handle_request(r, address)))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

