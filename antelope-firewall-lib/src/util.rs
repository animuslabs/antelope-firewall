use http_body_util::BodyExt;
use http_body_util::{combinators::BoxBody, Full};
use hyper::{body::Bytes, Response};

pub fn get_ratelimit_response(retry_after: u64) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(429)
        .header("Retry-After", retry_after)
        .header("Content-Type", "text/plain")
        .body(full(""))
        .expect("Invalid Ratelimit Response")
}

pub fn get_blocked_response() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(403)
        .header("Content-Type", "text/plain")
        .body(full(""))
        .expect("Invalid Blocked Response")
}

pub fn get_options_response() -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(200)
        .header("Allow", "OPTIONS, GET, POST")
        .body(full(""))
        .expect("Invalid Options Response")
}

pub fn get_error_response(
    body: BoxBody<Bytes, hyper::Error>,
) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(500)
        .header("Content-Type", "text/plain")
        .body(body)
        .expect("Invalid Error Response")
}

pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ratelimit_response_valid() {
        assert_eq!(get_ratelimit_response(0).status(), 429);
    }

    #[tokio::test]
    async fn blocked_response_valid() {
        assert_eq!(get_blocked_response().status(), 403);
    }

    #[tokio::test]
    async fn options_response_valid() {
        assert_eq!(get_options_response().status(), 200);
    }

    #[tokio::test]
    async fn error_response_valid() {
        assert_eq!(get_error_response(full("Internal Server Error")).status(), 500);
    }
}