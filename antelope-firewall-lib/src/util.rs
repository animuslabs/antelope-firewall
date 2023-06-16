use http_body_util::{combinators::BoxBody, Full};
use hyper::{Response, body::Bytes};
use http_body_util::BodyExt;

pub fn get_ratelimit_response(retry_after: u64) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(429)
        .header("Retry-After", retry_after)
        .header("Content-Type", "text/plain")
        .body(full("")).expect("Invalid Ratelimit Response")
}

pub fn get_blocked_response() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(403)
        .header("Content-Type", "text/plain")
        .body(full("")).expect("Invalid Ratelimit Response")
}

pub fn get_error_response(body: BoxBody<Bytes, hyper::Error>) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(500)
        .header("Content-Type", "text/plain")
        .body(body).expect("Invalid Error Response")
}

pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}
