use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

use crate::config::Config;
use crate::market_discovery::parse_gamma_event_response;
use crate::types::MarketWindow;

/// The user-facing result for one probed endpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeResult {
    pub name: &'static str,
    pub url: String,
    pub connect_ms: f64,
    pub first_message_ms: Option<f64>,
    pub message_age_ms: Option<f64>,
    pub details: Option<String>,
}

/// Run the end-to-end latency probe against the configured endpoints.
pub async fn run_latency_probe(config: &Config, timeout_ms: u64) -> anyhow::Result<()> {
    let timeout_duration = Duration::from_millis(timeout_ms);
    let gamma_client = crate::http::polymarket_http_client_builder()
        .timeout(timeout_duration)
        .build()
        .context("building HTTP client for latency probe")?;
    let mut results = Vec::new();
    let probe_window =
        discover_probe_window(&gamma_client, &config.gamma_api_url, timeout_duration).await?;

    results.push(probe_gamma(&gamma_client, &config.gamma_api_url, timeout_duration).await?);
    results.push(probe_websocket("binance", &config.binance_ws_url, None, timeout_duration).await?);
    results.push(
        probe_websocket(
            "rtds",
            &config.rtds_ws_url,
            Some(rtds_subscription_message()),
            timeout_duration,
        )
        .await?,
    );
    results.push(
        probe_websocket(
            "clob",
            &config.clob_ws_url,
            probe_window.as_ref().map(clob_subscription_message),
            timeout_duration,
        )
        .await?,
    );

    print_results(timeout_ms, &results);

    Ok(())
}

/// Discover the current or next BTC 5-minute market window for `CLOB` probing.
async fn discover_probe_window(
    client: &reqwest::Client,
    gamma_api_url: &str,
    timeout_duration: Duration,
) -> anyhow::Result<Option<MarketWindow>> {
    for slug in candidate_probe_slugs() {
        let url = format!("{gamma_api_url}/events/slug/{slug}");
        let Ok(Ok(response)) = timeout(timeout_duration, client.get(&url).send()).await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }

        let Ok(Ok(body)) = timeout(timeout_duration, response.json::<serde_json::Value>()).await
        else {
            continue;
        };

        if let Some(window) = parse_gamma_event_response(&body) {
            return Ok(Some(window));
        }
    }

    Ok(None)
}

/// Measure the current Gamma event-discovery request path.
async fn probe_gamma(
    client: &reqwest::Client,
    gamma_api_url: &str,
    timeout_duration: Duration,
) -> anyhow::Result<ProbeResult> {
    let slug = candidate_probe_slugs()
        .into_iter()
        .next()
        .context("latency probe did not produce candidate slugs")?;
    let url = format!("{gamma_api_url}/events/slug/{slug}");
    let started = Instant::now();
    let response = timeout(timeout_duration, client.get(&url).send())
        .await
        .context("Gamma probe timed out")?
        .context("Gamma probe request failed")?;
    let connect_ms = started.elapsed().as_secs_f64() * 1000.0;
    let status = response.status();

    let _ = timeout(timeout_duration, response.bytes())
        .await
        .context("Gamma probe body timed out")?
        .context("Gamma probe body read failed")?;

    Ok(ProbeResult {
        name: "gamma",
        url,
        connect_ms,
        first_message_ms: None,
        message_age_ms: None,
        details: Some(format!("status={status} slug={slug}")),
    })
}

/// Measure the websocket handshake, subscription, and first-message latency.
async fn probe_websocket(
    name: &'static str,
    url: &str,
    subscription: Option<String>,
    timeout_duration: Duration,
) -> anyhow::Result<ProbeResult> {
    let connect_started = Instant::now();
    let (mut socket, _response) = timeout(timeout_duration, tokio_tungstenite::connect_async(url))
        .await
        .with_context(|| format!("{name} websocket connect timed out"))?
        .with_context(|| format!("{name} websocket connect failed"))?;
    let connect_ms = connect_started.elapsed().as_secs_f64() * 1000.0;

    let wait_started = Instant::now();
    if let Some(subscription) = subscription {
        timeout(
            timeout_duration,
            socket.send(Message::Text(subscription.into())),
        )
        .await
        .with_context(|| format!("{name} subscription send timed out"))?
        .with_context(|| format!("{name} subscription send failed"))?;
    }

    let (first_message_ms, message_age_ms, details) =
        read_first_probe_message(name, &mut socket, timeout_duration, wait_started).await?;

    let _ = socket.close(None).await;

    Ok(ProbeResult {
        name,
        url: url.to_string(),
        connect_ms,
        first_message_ms,
        message_age_ms,
        details,
    })
}

/// Read the first text message useful for operator latency measurements.
async fn read_first_probe_message(
    name: &'static str,
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    timeout_duration: Duration,
    wait_started: Instant,
) -> anyhow::Result<(Option<f64>, Option<f64>, Option<String>)> {
    loop {
        let message = timeout(timeout_duration, socket.next())
            .await
            .with_context(|| format!("{name} first message timed out"))?
            .context("websocket stream ended before first probe message")?
            .with_context(|| format!("{name} websocket read failed"))?;

        match message {
            Message::Text(text) => {
                let first_message_ms = Some(wait_started.elapsed().as_secs_f64() * 1000.0);
                let json = serde_json::from_str::<serde_json::Value>(&text).ok();
                let message_age_ms =
                    json.as_ref()
                        .and_then(extract_source_timestamp_us)
                        .map(|event_us| {
                            let now_us = crate::feeds::util::now_us();
                            now_us.saturating_sub(event_us) as f64 / 1000.0
                        });
                let details = json
                    .as_ref()
                    .and_then(|value| summarize_probe_message(value, text.len()));
                return Ok((first_message_ms, message_age_ms, details));
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .with_context(|| format!("{name} pong failed during probe"))?;
            }
            Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
            Message::Close(frame) => {
                let reason =
                    frame.map_or_else(|| "closed".to_string(), |frame| frame.reason.to_string());
                return Ok((None, None, Some(reason)));
            }
        }
    }
}

/// Return the candidate current and next five-minute BTC market slugs.
fn candidate_probe_slugs() -> Vec<String> {
    let epoch_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let current_slot = (epoch_secs / 300) * 300;
    vec![
        format!("btc-updown-5m-{current_slot}"),
        format!("btc-updown-5m-{}", current_slot + 300),
    ]
}

/// Build the RTDS subscription payload used in live paper mode.
fn rtds_subscription_message() -> String {
    serde_json::json!({
        "action": "subscribe",
        "subscriptions": [{
            "topic": "crypto_prices_chainlink",
            "type": "*",
            "filters": "{\"symbol\":\"btc/usd\"}"
        }]
    })
    .to_string()
}

/// Build the `CLOB` market subscription for the probed BTC window.
fn clob_subscription_message(window: &MarketWindow) -> String {
    serde_json::json!({
        "type": "market",
        "assets_ids": [&window.up_token_id, &window.down_token_id],
        "custom_feature_enabled": true,
    })
    .to_string()
}

/// Extract a source-event timestamp from known websocket payload shapes.
fn extract_source_timestamp_us(value: &serde_json::Value) -> Option<u64> {
    value
        .get("data")
        .and_then(extract_source_timestamp_us)
        .or_else(|| {
            value
                .get("payload")
                .and_then(extract_source_timestamp_us_from_payload)
        })
        .or_else(|| extract_timestamp_field(value, &["E", "timestamp", "ts", "time"]))
}

/// Extract a source-event timestamp from nested payload shapes.
fn extract_source_timestamp_us_from_payload(value: &serde_json::Value) -> Option<u64> {
    value
        .get("data")
        .and_then(|entries| entries.as_array())
        .and_then(|entries| entries.first())
        .and_then(|entry| extract_timestamp_field(entry, &["timestamp", "ts", "time"]))
        .or_else(|| extract_timestamp_field(value, &["timestamp", "ts", "time"]))
}

/// Parse one of the candidate timestamp fields and normalize it to microseconds.
fn extract_timestamp_field(value: &serde_json::Value, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(parse_u64_value)
            .map(normalize_epoch_to_us)
    })
}

/// Parse a numeric `JSON` field into `u64`.
fn parse_u64_value(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

/// Normalize millisecond or microsecond epoch values to microseconds.
fn normalize_epoch_to_us(value: u64) -> u64 {
    if value >= 10_000_000_000_000 {
        value
    } else {
        value.saturating_mul(1000)
    }
}

/// Summarize the first probe message for operator output.
fn summarize_probe_message(value: &serde_json::Value, raw_len: usize) -> Option<String> {
    value
        .get("stream")
        .and_then(serde_json::Value::as_str)
        .map(|stream| format!("stream={stream} bytes={raw_len}"))
        .or_else(|| {
            value
                .get("topic")
                .and_then(serde_json::Value::as_str)
                .map(|topic| format!("topic={topic} bytes={raw_len}"))
        })
        .or_else(|| {
            value
                .get("event_type")
                .and_then(serde_json::Value::as_str)
                .map(|event_type| format!("event_type={event_type} bytes={raw_len}"))
        })
        .or_else(|| Some(format!("bytes={raw_len}")))
}

/// Print probe results in a stable operator-friendly format.
fn print_results(timeout_ms: u64, results: &[ProbeResult]) {
    println!("Latency probe timeout={timeout_ms}ms");
    for result in results {
        let first_message = result
            .first_message_ms
            .map_or_else(|| "-".to_string(), |value| format!("{value:.1}"));
        let message_age = result
            .message_age_ms
            .map_or_else(|| "-".to_string(), |value| format!("{value:.1}"));
        let details = result.details.as_deref().unwrap_or("-");
        println!(
            "{:<8} connect_ms={:<8.1} first_message_ms={:<8} message_age_ms={:<8} {}",
            result.name, result.connect_ms, first_message, message_age, details
        );
        println!("         {}", result.url);
    }
}

#[cfg(test)]
#[path = "tests/latency_probe_tests.rs"]
mod tests;
