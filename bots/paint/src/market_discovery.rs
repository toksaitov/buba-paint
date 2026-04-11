use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::config::Config;
use crate::types::MarketWindow;

/// Events emitted by the market discovery task.
#[derive(Debug)]
pub enum MarketDiscoveryEvent {
    /// A new 5-minute window has been discovered and is now active.
    NewWindow(MarketWindow),
    /// A market window has reached its end time and is now closed.
    WindowClosed(MarketWindow),
}

/// Handle for receiving market discovery events.
pub struct MarketDiscoveryHandle {
    pub window_rx: mpsc::Receiver<MarketDiscoveryEvent>,
}

/// Launch the market discovery polling loop as a background tokio task.
///
/// Polls the Gamma API at `config.gamma_poll_interval` ms for BTC 5-minute
/// up/down markets.  Returns a `MarketDiscoveryHandle` whose `window_rx`
/// channel emits `NewWindow` and `WindowClosed` events.
#[allow(clippy::unused_async)]
pub async fn run_market_discovery(config: &Config) -> MarketDiscoveryHandle {
    let (tx, rx) = mpsc::channel::<MarketDiscoveryEvent>(32);

    let gamma_api_url = config.gamma_api_url.clone();
    let clob_api_url = config.clob_api_url.clone();
    let poll_interval_ms = config.gamma_poll_interval;

    tokio::spawn(async move {
        discovery_loop(gamma_api_url, clob_api_url, poll_interval_ms, tx).await;
    });

    MarketDiscoveryHandle { window_rx: rx }
}

/// Internal polling loop.
///
/// Every `poll_interval_ms` it computes the current and next 5-minute window
/// slugs, fetches them from the Gamma API, and emits `NewWindow` events for
/// any newly-discovered windows.  It also schedules `WindowClosed` events
/// via `tokio::time::sleep` when a window's `end_time` arrives.
async fn discovery_loop(
    gamma_api_url: String,
    clob_api_url: String,
    poll_interval_ms: u64,
    tx: mpsc::Sender<MarketDiscoveryEvent>,
) {
    let mut seen_slugs: HashSet<String> = HashSet::new();
    let client = reqwest::Client::new();
    let mut interval = tokio::time::interval(Duration::from_millis(poll_interval_ms));

    loop {
        interval.tick().await;

        let epoch_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let current_slot = (epoch_secs / 300) * 300;
        let next_slot = current_slot + 300;

        for slot in [current_slot, next_slot] {
            let slug = format!("btc-updown-5m-{slot}");

            if seen_slugs.contains(&slug) {
                continue;
            }

            let url = format!("{gamma_api_url}/events/slug/{slug}");
            info!(discovery = "gamma", "fetching {url}");

            match fetch_market(&client, &url, &clob_api_url).await {
                Ok(Some(window)) => {
                    info!(
                        discovery = "gamma",
                        slug = %window.slug,
                        market_id = %window.market_id,
                        "discovered new window"
                    );
                    seen_slugs.insert(slug);

                    let close_window = window.clone();
                    let close_tx = tx.clone();
                    let epoch_ms = epoch_secs * 1000;
                    let end_ms = window.end_time;
                    if end_ms > epoch_ms {
                        tokio::spawn(async move {
                            let delay = Duration::from_millis(end_ms - epoch_ms);
                            tokio::time::sleep(delay).await;
                            let _ = close_tx
                                .send(MarketDiscoveryEvent::WindowClosed(close_window))
                                .await;
                        });
                    }

                    if tx
                        .send(MarketDiscoveryEvent::NewWindow(window))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(discovery = "gamma", "fetch failed for {slug}: {e}");
                }
            }
        }
    }
}

/// Fetch a single market event from the Gamma API and parse it into a
/// `MarketWindow`, or return `None` if the event is not found / not suitable.
async fn fetch_market(
    client: &reqwest::Client,
    url: &str,
    clob_api_url: &str,
) -> anyhow::Result<Option<MarketWindow>> {
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        anyhow::bail!("HTTP {}: {url}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    let Some(window) = parse_gamma_event_response(&body) else {
        return Ok(None);
    };
    Ok(Some(
        augment_market_with_token_fee_rates(client, clob_api_url, window).await,
    ))
}

/// Parse a Gamma API event response into a `MarketWindow`.
///
/// Pure function -- takes already-parsed `JSON`, no `HTTP`.
/// Returns `None` when the response is missing required fields or when all
/// candidate markets fail validation (missing outcomes, token IDs, etc.).
pub(crate) fn parse_gamma_event_response(body: &serde_json::Value) -> Option<MarketWindow> {
    if body.get("markets").is_none() && body.get("id").is_none() {
        return None;
    }
    gamma_market_candidates(body)
        .into_iter()
        .find_map(|market| parse_gamma_market_candidate(body, market))
}

/// Parse a flexible numeric Gamma field into `f64`.
fn parse_f64_value(val: Option<&serde_json::Value>) -> Option<f64> {
    val.and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_i64().map(|n| n as f64))
            .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

/// Return the market candidates embedded in a Gamma event payload.
fn gamma_market_candidates(body: &serde_json::Value) -> Vec<&serde_json::Value> {
    let markets = body
        .get("markets")
        .and_then(|value| value.as_array())
        .map_or_else(Vec::new, |markets| markets.iter().collect());
    if markets.is_empty() {
        vec![body]
    } else {
        markets
    }
}

/// Parse one candidate market entry from a Gamma event payload.
fn parse_gamma_market_candidate(
    event: &serde_json::Value,
    market: &serde_json::Value,
) -> Option<MarketWindow> {
    let market_id = market_id(market)?;
    let condition_id = market
        .get("conditionId")
        .or_else(|| market.get("condition_id"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let slug = event
        .get("slug")
        .and_then(|value| value.as_str())
        .or_else(|| market.get("slug").and_then(|value| value.as_str()))
        .unwrap_or("")
        .to_string();
    let (up_token_id, down_token_id) = gamma_token_pair(market)?;
    let end_time = parse_end_date(
        market
            .get("endDate")
            .or_else(|| market.get("end_date"))
            .or_else(|| event.get("endDate"))
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    )
    .unwrap_or(0);

    Some(MarketWindow {
        market_id,
        question: market
            .get("question")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        up_token_id,
        down_token_id,
        condition_id,
        start_time: end_time.saturating_sub(300_000),
        end_time,
        slug,
        outcome: None,
        resolution_source: string_field(market, event, "resolutionSource"),
        fee_profile: Some("crypto".to_string()),
        order_min_size: number_field(market, event, "orderMinSize"),
        order_price_min_tick_size: number_field(market, event, "orderPriceMinTickSize"),
        maker_base_fee: number_field(market, event, "makerBaseFee"),
        taker_base_fee: number_field(market, event, "takerBaseFee"),
        rewards_min_size: number_field(market, event, "rewardsMinSize"),
        rewards_max_spread: number_field(market, event, "rewardsMaxSpread"),
        fees_enabled: bool_field(market, event, "feesEnabled"),
        fee_schedule_json: json_field(market, event, "feeSchedule"),
        token_fee_rates_json: None,
        accepting_orders: bool_field(market, event, "acceptingOrders"),
        accepting_orders_timestamp: string_field(market, event, "acceptingOrdersTimestamp"),
        clear_book_on_start: bool_field(market, event, "clearBookOnStart"),
    })
}

/// Extract the canonical market identifier from a Gamma market entry.
fn market_id(market: &serde_json::Value) -> Option<String> {
    let market_id = market
        .get("id")
        .or_else(|| market.get("conditionId"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    (!market_id.is_empty()).then_some(market_id)
}

/// Extract the up/down token ids from a Gamma market entry.
fn gamma_token_pair(market: &serde_json::Value) -> Option<(String, String)> {
    let outcomes = parse_string_or_array(market.get("outcomes"));
    let clob_token_ids = parse_string_or_array(market.get("clobTokenIds"));
    if outcomes.len() < 2 || clob_token_ids.len() < 2 {
        return None;
    }
    let (up_idx, down_idx) = gamma_outcome_indices(&outcomes);
    if up_idx >= clob_token_ids.len() || down_idx >= clob_token_ids.len() {
        return None;
    }
    Some((
        clob_token_ids[up_idx].clone(),
        clob_token_ids[down_idx].clone(),
    ))
}

/// Resolve the outcome indices for the up/down tokens.
fn gamma_outcome_indices(outcomes: &[String]) -> (usize, usize) {
    let up_idx = outcomes.iter().position(|outcome| {
        let lower = outcome.to_lowercase();
        lower == "up" || lower == "yes"
    });
    let down_idx = outcomes.iter().position(|outcome| {
        let lower = outcome.to_lowercase();
        lower == "down" || lower == "no"
    });
    match (up_idx, down_idx) {
        (Some(up_idx), Some(down_idx)) => (up_idx, down_idx),
        _ => (0, 1),
    }
}

/// Read a string field from the market entry with event-level fallback.
fn string_field(
    market: &serde_json::Value,
    event: &serde_json::Value,
    key: &str,
) -> Option<String> {
    market
        .get(key)
        .or_else(|| event.get(key))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

/// Read a numeric field from the market entry with event-level fallback.
fn number_field(market: &serde_json::Value, event: &serde_json::Value, key: &str) -> Option<f64> {
    parse_f64_value(market.get(key).or_else(|| event.get(key)))
}

/// Read one boolean field from the market entry with event-level fallback.
fn bool_field(market: &Value, event: &Value, key: &str) -> Option<bool> {
    market
        .get(key)
        .or_else(|| event.get(key))
        .and_then(Value::as_bool)
}

/// Serialize one nested JSON field from the market entry with event-level fallback.
fn json_field(market: &Value, event: &Value, key: &str) -> Option<String> {
    market
        .get(key)
        .or_else(|| event.get(key))
        .and_then(|value| serde_json::to_string(value).ok())
}

/// Fetch and attach the current token fee-rate payloads for both market sides.
async fn augment_market_with_token_fee_rates(
    client: &reqwest::Client,
    clob_api_url: &str,
    mut window: MarketWindow,
) -> MarketWindow {
    let mut token_rates = serde_json::Map::new();
    for (label, token_id) in [
        ("up", window.up_token_id.clone()),
        ("down", window.down_token_id.clone()),
    ] {
        let url = format!("{clob_api_url}/fee-rate?token_id={token_id}");
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
                Ok(value) => {
                    token_rates.insert(label.to_string(), value);
                }
                Err(error) => {
                    warn!(market_id = %window.market_id, %token_id, "failed to decode fee-rate response: {error}");
                }
            },
            Ok(resp) => {
                warn!(
                    market_id = %window.market_id,
                    %token_id,
                    status = %resp.status(),
                    "fee-rate endpoint returned non-success"
                );
            }
            Err(error) => {
                warn!(market_id = %window.market_id, %token_id, "fee-rate request failed: {error}");
            }
        }
    }

    if !token_rates.is_empty() {
        window.token_fee_rates_json = serde_json::to_string(&Value::Object(token_rates)).ok();
    }

    window
}

/// Parse a `JSON` value that may be a string (`JSON`-encoded array) or an actual
/// `JSON` array of strings.
pub(crate) fn parse_string_or_array(val: Option<&serde_json::Value>) -> Vec<String> {
    let Some(val) = val else {
        return Vec::new();
    };

    if let Some(arr) = val.as_array() {
        return arr
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect();
    }

    if let Some(s) = val.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
            return parsed;
        }
        if !s.is_empty() {
            return vec![s.to_string()];
        }
    }

    Vec::new()
}

/// Parse an ISO 8601 date string into milliseconds since epoch.
pub(crate) fn parse_end_date(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis() as u64);
    }

    let s_utc = if s.contains('Z') || s.contains('+') {
        s.to_string()
    } else {
        format!("{s}Z")
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s_utc) {
        return Some(dt.timestamp_millis() as u64);
    }

    None
}

/// Poll the Gamma API for a market's resolved outcome.
///
/// Retries up to `max_retries` times with `delay_ms` between attempts.
/// Returns `Some(SignalDirection)` when the market is resolved, or `None`
/// if resolution could not be determined after all retries.
pub async fn fetch_resolution_once(
    gamma_api_url: &str,
    slug: &str,
) -> anyhow::Result<Option<crate::types::SignalDirection>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(10_000))
        .connect_timeout(std::time::Duration::from_millis(5_000))
        .build()
        .unwrap_or_default();
    let url = format!("{gamma_api_url}/events/slug/{slug}");

    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("resolution poll HTTP {} for {slug}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    let direction =
        crate::verify::parse_gamma_outcome(&body).and_then(|outcome| match outcome.as_str() {
            "UP" => Some(crate::types::SignalDirection::Up),
            "DOWN" => Some(crate::types::SignalDirection::Down),
            _ => None,
        });
    Ok(direction)
}

/// Poll the Gamma API for a market's resolved outcome.
///
/// Retries up to `max_retries` times with `delay_ms` between attempts.
/// Returns `Some(SignalDirection)` when the market is resolved, or `None`
/// if resolution could not be determined after all retries.
pub async fn poll_resolution(
    gamma_api_url: &str,
    slug: &str,
    max_retries: u32,
    delay_ms: u64,
) -> Option<crate::types::SignalDirection> {
    for attempt in 0..=max_retries {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }

        match fetch_resolution_once(gamma_api_url, slug).await {
            Ok(Some(direction)) => {
                info!(
                    slug,
                    ?direction,
                    attempt,
                    "resolution confirmed via Gamma API"
                );
                return Some(direction);
            }
            Ok(None) => {}
            Err(e) => {
                warn!(slug, attempt, "resolution poll failed: {e}");
                continue;
            }
        }

        if attempt < max_retries {
            tracing::debug!(slug, attempt, "market not yet resolved, retrying...");
        }
    }

    warn!(
        slug,
        max_retries, "resolution polling exhausted, market not resolved"
    );
    None
}

#[cfg(test)]
#[path = "tests/market_discovery_tests.rs"]
mod tests;
