use std::net::SocketAddr;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use tokio::net::TcpListener;

use toml::Table;

lazy_static::lazy_static! {
    static ref CONFIG: Table = {
        std::fs::read_to_string("test/example.toml").unwrap().parse::<Table>().unwrap()
    };
}

async fn hello(req: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, String> {
    
    let client = reqwest::Client::new();
    let possible_nodes = CONFIG.get("nodes").unwrap().as_array().unwrap();
    let res = client.get(format!(
        "http://{}/{}",
        possible_nodes[rand::random::<usize>() % possible_nodes.len()].as_str().unwrap(),
        req.uri().path(),
    ))
        .send()
        .await.map_err(|e| e.to_string())?;
    Ok(Response::builder()
       .status(200)
       .header("Content-Type", "application/json")
       .body(Full::new(Bytes::from(res.text().await.map_err(|e| e.to_string())?)))
       .unwrap()
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(stream, service_fn(hello))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

