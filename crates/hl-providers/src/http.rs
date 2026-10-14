//! One HTTP client for every engine: pooled connections (HTTP/2 where the
//! server offers it), so the second request skips the TLS handshake.

use std::time::Duration;

use reqwest::{Client, StatusCode};

use crate::EngineError;

/// The shared client. Cheap to clone (it's an `Arc` inside).
pub fn client() -> Client {
    Client::builder()
        .user_agent(concat!("HeliLingo/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .gzip(true)
        // Translate endpoints don't redirect; a redirect is Google's
        // "unusual traffic" page, which the engine reads as busy.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("TLS backend available")
}

/// A transport failure: no connection, DNS, TLS, reset.
pub fn network(e: reqwest::Error) -> EngineError {
    if e.is_timeout() {
        EngineError::Timeout
    } else {
        EngineError::Network(short(&e))
    }
}

/// A failed status. `keyed`: the request carried an API key, so 401/403 is
/// about the key; keyless endpoints answer 403 when they throttle.
pub fn status(status: StatusCode, keyed: bool) -> EngineError {
    match status.as_u16() {
        401 | 403 if keyed => EngineError::Unauthorized,
        402 | 456 => EngineError::PaymentRequired,
        401 | 403 | 429 => EngineError::RateLimited,
        408 | 504 => EngineError::Timeout,
        s => EngineError::Server(format!("HTTP {s}")),
    }
}

/// The innermost cause, which never has the URL (it may carry a query with
/// the user's text).
fn short(e: &reqwest::Error) -> String {
    let mut msg = if e.is_connect() { "connection failed".to_owned() } else { "request failed".to_owned() };
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        msg = s.to_string();
        src = s.source();
    }
    msg
}
