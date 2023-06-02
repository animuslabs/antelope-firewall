use std::net::SocketAddr;

use http_body_util::{Full, BodyExt};
use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, Method};
use tokio::net::TcpListener;

mod router;
use router::*;

lazy_static::lazy_static! {
    static ref ROUTER: Router = Router::new();
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn handle_request(req: Request<hyper::body::Incoming>, request_number: usize) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    match req.method() {
        &Method::POST => {
            let (parts, body) = req.into_parts();
            println!("Handling request: {}", parts.uri.path());

            let possible_nodes: Vec<String> = ROUTER.get_nodes_for_path(parts.uri.path());
            if possible_nodes.len() == 0 {
                println!("No nodes available for {}", parts.uri.path());
                return Ok(Response::new(full("No nodes available for this path")));
            }

            let client = reqwest::Client::new();

            let body_bytes = body.collect().await?.to_bytes();
            let node_res = client.post(format!(
                "{}/{}",
                possible_nodes[request_number % possible_nodes.len()],
                parts.uri.path(),
            ))
                .headers(parts.headers.clone())
                .body(body_bytes)
                .send()
                .await.unwrap();

            println!("Sending request to node: {}", possible_nodes[request_number % possible_nodes.len()]);

            let mut client_res = Response::builder()
                .status(node_res.status());

            client_res.headers_mut().map(|h| h.clone_from(node_res.headers()));
            Ok(client_res.body(full(node_res.bytes().await.unwrap())).unwrap())
        },
        _ => {
            Ok(Response::new(full("Invalid Method")))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;
    let mut request_num = 0;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;
        let this_request = request_num;

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(stream, service_fn(|r| handle_request(r, this_request)))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
        request_num += 1;
    }
}

