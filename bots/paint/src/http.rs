use anyhow::Context;

/// Stable user agent used for Polymarket and Gamma HTTP requests.
pub const POLYMARKET_HTTP_USER_AGENT: &str = concat!("buba-paint/", env!("CARGO_PKG_VERSION"));

/// Return a reqwest client builder with venue-safe default request headers.
pub fn polymarket_http_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().user_agent(POLYMARKET_HTTP_USER_AGENT)
}

/// Build a Polymarket/Gamma HTTP client with the repository default headers.
pub fn polymarket_http_client() -> anyhow::Result<reqwest::Client> {
    polymarket_http_client_builder()
        .build()
        .context("building Polymarket HTTP client")
}
