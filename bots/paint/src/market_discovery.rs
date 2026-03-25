// Market discovery — polls Gamma API for 5-minute BTC Up/Down markets.

use std::collections::HashSet;
use std::time::Duration;

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
    let poll_interval_ms = config.gamma_poll_interval;

    tokio::spawn(async move {
        discovery_loop(gamma_api_url, poll_interval_ms, tx).await;
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

        // Current window: floor(now / 300) * 300.
        let current_slot = (epoch_secs / 300) * 300;
        // Next window: current + 300.
        let next_slot = current_slot + 300;

        for slot in [current_slot, next_slot] {
            let slug = format!("btc-updown-5m-{slot}");

            if seen_slugs.contains(&slug) {
                continue;
            }

            let url = format!("{gamma_api_url}/events/slug/{slug}");
            info!(discovery = "gamma", "fetching {url}");

            match fetch_market(&client, &url).await {
                Ok(Some(window)) => {
                    info!(
                        discovery = "gamma",
                        slug = %window.slug,
                        market_id = %window.market_id,
                        "discovered new window"
                    );
                    seen_slugs.insert(slug);

                    // Schedule a WindowClosed event at end_time.
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
                        // Receiver dropped — shut down.
                        return;
                    }
                }
                Ok(None) => {
                    // Market not found yet — will retry next poll.
                }
                Err(e) => {
                    warn!(discovery = "gamma", "fetch failed for {slug}: {e}");
                }
            }
        }
    }
}

/// Fetch a single market event from the Gamma API and parse it into a
/// `MarketWindow`, or return `None` if the event is not found / not suitable.
async fn fetch_market(client: &reqwest::Client, url: &str) -> anyhow::Result<Option<MarketWindow>> {
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        // 404 is expected when a window hasn't been created yet.
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        anyhow::bail!("HTTP {}: {url}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    Ok(parse_gamma_event_response(&body))
}

/// Parse a Gamma API event response into a `MarketWindow`.
///
/// Pure function -- takes already-parsed JSON, no HTTP.
/// Returns `None` when the response is missing required fields or when all
/// candidate markets fail validation (missing outcomes, token IDs, etc.).
pub(crate) fn parse_gamma_event_response(body: &serde_json::Value) -> Option<MarketWindow> {
    // The response may be the event object directly or wrapped in a field.
    // We need either a "markets" array or an "id" field to proceed.
    if body.get("markets").is_none() && body.get("id").is_none() {
        return None;
    }
    let event = body;

    // Find the first market inside the event that has outcomes we can parse.
    let markets = event
        .get("markets")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    // If no "markets" array, treat the event itself as the market.
    let candidates: Vec<&serde_json::Value> = if markets.is_empty() {
        vec![event]
    } else {
        markets.iter().collect()
    };

    for market in candidates {
        let market_id = market
            .get("id")
            .or_else(|| market.get("conditionId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if market_id.is_empty() {
            continue;
        }

        let question = market
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let condition_id = market
            .get("conditionId")
            .or_else(|| market.get("condition_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let slug = event
            .get("slug")
            .and_then(|v| v.as_str())
            .or_else(|| market.get("slug").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();

        // Parse outcomes — may be a JSON array of strings or a single string.
        let outcomes = parse_string_or_array(market.get("outcomes"));
        // Parse clobTokenIds — same flexible format.
        let clob_token_ids = parse_string_or_array(market.get("clobTokenIds"));

        if outcomes.len() < 2 || clob_token_ids.len() < 2 {
            continue;
        }

        // Find UP and DOWN indices.
        let up_idx = outcomes.iter().position(|o| {
            let lower = o.to_lowercase();
            lower == "up" || lower == "yes"
        });
        let down_idx = outcomes.iter().position(|o| {
            let lower = o.to_lowercase();
            lower == "down" || lower == "no"
        });

        let (up_idx, down_idx) = match (up_idx, down_idx) {
            (Some(u), Some(d)) => (u, d),
            _ => {
                // Fall back to positional: first = UP, second = DOWN.
                (0, 1)
            }
        };

        if up_idx >= clob_token_ids.len() || down_idx >= clob_token_ids.len() {
            continue;
        }

        let up_token_id = clob_token_ids[up_idx].clone();
        let down_token_id = clob_token_ids[down_idx].clone();

        // Parse end_date to get end_time.
        let end_date_str = market
            .get("endDate")
            .or_else(|| market.get("end_date"))
            .or_else(|| event.get("endDate"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let end_time = parse_end_date(end_date_str).unwrap_or(0);

        // Start time is end_time - 300_000 (5 minutes).
        let start_time = end_time.saturating_sub(300_000);

        return Some(MarketWindow {
            market_id,
            question,
            up_token_id,
            down_token_id,
            condition_id,
            start_time,
            end_time,
            slug,
        });
    }

    None
}

/// Parse a JSON value that may be a string (JSON-encoded array) or an actual
/// JSON array of strings.
pub(crate) fn parse_string_or_array(val: Option<&serde_json::Value>) -> Vec<String> {
    let Some(val) = val else {
        return Vec::new();
    };

    // If it's already an array, extract string elements.
    if let Some(arr) = val.as_array() {
        return arr
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect();
    }

    // If it's a string, try to parse it as JSON.
    if let Some(s) = val.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
            return parsed;
        }
        // Could be a single value (not an array).
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

    // Try RFC 3339 first.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis() as u64);
    }

    // Try without timezone (assume UTC).
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

#[cfg(test)]
#[path = "tests/market_discovery_tests.rs"]
mod tests;
