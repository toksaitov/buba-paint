mod support;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use buba_paint::config::Config;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::mock_ws::MockWsServer;

/// Helper: build a default test config wired to the given mock server URLs.
fn test_config(binance_url: &str, clob_url: &str, chainlink_url: &str, gamma_url: &str) -> Config {
    Config {
        binance_ws_url: binance_url.to_string(),
        clob_ws_url: clob_url.to_string(),
        rtds_ws_url: chainlink_url.to_string(),
        gamma_api_url: gamma_url.to_string(),
        clob_api_url: gamma_url.to_string(),
        gamma_poll_interval: 500,
        tick_interval: 200,
        clob_ping_interval: 30_000,
        rtds_ping_interval: 30_000,
        chainlink_stale_ms: 10_000,
        reconnect_base_delay: 100,
        reconnect_max_delay: 500,
        latency_arb_momentum_threshold: 0.001,
        latency_arb_max_ask: 0.60,
        latency_arb_min_ask: 0.20,
        latency_arb_cooldown_ms: 100,
        min_window_time_ms: 1_000,
        starting_balance: 200.0,
        peak_dd_pause_pct: 1.0,
        spread_capture_threshold: 0.50,
        momentum_window_ms: 5_000,
        log_level: "warn".to_string(),
        resolution_poll_retries: 0,
        resolution_initial_delay_ms: 0,
        resolution_poll_delay_ms: 0,
        ..Config::default()
    }
}

/// Gamma market response.
fn gamma_market_response(
    slug: String,
    market_id: &str,
    condition_id: &str,
    up_token: &str,
    down_token: &str,
    end_date: &str,
    outcome: Option<&str>,
) -> serde_json::Value {
    let mut event = serde_json::json!({
        "slug": slug,
        "markets": [{
            "id": market_id,
            "question": "Will BTC go up?",
            "conditionId": condition_id,
            "outcomes": ["Up", "Down"],
            "clobTokenIds": [up_token, down_token],
            "endDate": end_date
        }]
    });

    let outcome_prices = match outcome {
        Some("UP") => Some(serde_json::json!(["1", "0"])),
        Some("DOWN") => Some(serde_json::json!(["0", "1"])),
        _ => None,
    };
    if let Some(outcome_prices) = outcome_prices {
        event["markets"][0]["outcomePrices"] = outcome_prices;
    }

    event
}

/// Mount gamma sequence.
async fn mount_gamma_sequence(
    gamma_mock: &MockServer,
    path_pattern: String,
    unresolved_body: serde_json::Value,
    resolved_body: serde_json::Value,
) {
    Mock::given(method("GET"))
        .and(path_regex(path_pattern.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(unresolved_body))
        .up_to_n_times(1)
        .mount(gamma_mock)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(path_pattern))
        .respond_with(ResponseTemplate::new(200).set_body_json(resolved_body))
        .mount(gamma_mock)
        .await;
}

/// Helper: register a Gamma API mock that returns a market ending at `end_date`
/// with the given `current_slot` slug and then resolves to the provided outcome
/// on later polls.
async fn register_gamma_mock_with_resolution(
    gamma_mock: &MockServer,
    current_slot: u64,
    end_date: &str,
    outcome: &str,
) {
    let slug = format!("btc-updown-5m-{current_slot}");
    let unresolved = gamma_market_response(
        slug.clone(),
        "mkt-sys-test",
        "cond-sys",
        "tok-up-sys",
        "tok-down-sys",
        end_date,
        None,
    );
    let resolved = gamma_market_response(
        slug,
        "mkt-sys-test",
        "cond-sys",
        "tok-up-sys",
        "tok-down-sys",
        end_date,
        Some(outcome),
    );
    mount_gamma_sequence(
        gamma_mock,
        r"^/events/slug/btc-updown-5m-\d+$".to_string(),
        unresolved,
        resolved,
    )
    .await;
}

/// Helper: register an unresolved Gamma discovery response.
async fn register_gamma_mock(gamma_mock: &MockServer, current_slot: u64, end_date: &str) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(gamma_market_response(
                format!("btc-updown-5m-{current_slot}"),
                "mkt-sys-test",
                "cond-sys",
                "tok-up-sys",
                "tok-down-sys",
                end_date,
                None,
            )),
        )
        .mount(gamma_mock)
        .await;
}

/// Helper: send a burst of rising Binance ticks to generate positive momentum.
async fn send_rising_binance_ticks(binance_mock: &MockWsServer, count: u32) {
    let base_price = 42_000.0;
    for i in 0..count {
        let price = base_price + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Helper: send a burst of flat Binance ticks that should not create strong momentum.
async fn send_flat_binance_ticks(binance_mock: &MockWsServer, count: u32) {
    let base_price = 42_000.0;
    for i in 0..count {
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_100_000_000_u64 + u64::from(i) * 100,
            base_price,
            1_700_100_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Helper: send `CLOB` book snapshots with a low UP ask (triggers latency-arb).
async fn send_clob_book(clob_mock: &MockWsServer) {
    let timestamp = current_test_ms();
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-up-sys","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.45","size":"100"}}]}}"#
        ))
        .await;
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-down-sys","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.50","size":"100"}}]}}"#
        ))
        .await;
}

/// Helper: send `CLOB` book snapshots without a source timestamp so the live
/// path must rely on observed freshness instead.
async fn send_clob_book_without_timestamp(clob_mock: &MockWsServer) {
    clob_mock
        .send(
            r#"{"asset_id":"tok-up-sys","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.45","size":"100"}]}"#,
        )
        .await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-down-sys","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.50","size":"100"}]}"#,
        )
        .await;
}

/// Helper: send direct best-bid-ask updates without size fields so the live
/// path must preserve the existing book liquidity.
async fn send_clob_best_bid_ask_without_sizes(clob_mock: &MockWsServer) {
    clob_mock
        .send(
            r#"{"event_type":"best_bid_ask","asset_id":"tok-up-sys","best_bid":"0.40","best_ask":"0.45"}"#,
        )
        .await;
    clob_mock
        .send(
            r#"{"event_type":"best_bid_ask","asset_id":"tok-down-sys","best_bid":"0.40","best_ask":"0.50"}"#,
        )
        .await;
}

/// Helper: wait until the replay feed table contains a sized `CLOB` snapshot.
async fn wait_for_persisted_clob_snapshot_with_sizes(db_path: &str) {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let conn = rusqlite::Connection::open(db_path).unwrap();
        let legacy_count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM feed_events
                 WHERE event_type = 'book'
                   AND source IN ('clob_up', 'clob_down')
                   AND ask_size > 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let compact_count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM clob_replay_events
                 WHERE event_type = 'book'
                   AND source IN ('clob_up', 'clob_down')
                   AND ask_size > 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if legacy_count + compact_count > 0 {
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Helper: send `CLOB` books that remain too expensive for spread-capture and latency-arb.
async fn send_high_ask_clob_book(clob_mock: &MockWsServer) {
    let timestamp = current_test_ms();
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-up-sys","timestamp":{timestamp},"bids":[{{"price":"0.10","size":"100"}}],"asks":[{{"price":"0.70","size":"100"}}]}}"#
        ))
        .await;
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-down-sys","timestamp":{timestamp},"bids":[{{"price":"0.10","size":"100"}}],"asks":[{{"price":"0.70","size":"100"}}]}}"#
        ))
        .await;
}

/// Helper: send a spread-capture candidate that is attractive on price but too
/// expensive to satisfy the per-leg minimum under a `$200` bankroll and `5%` cap.
async fn send_unaffordable_spread_clob_book(clob_mock: &MockWsServer) {
    let timestamp = current_test_ms();
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-up-sys","timestamp":{timestamp},"bids":[{{"price":"0.45","size":"100"}}],"asks":[{{"price":"0.46","size":"100"}}]}}"#
        ))
        .await;
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-down-sys","timestamp":{timestamp},"bids":[{{"price":"0.49","size":"100"}}],"asks":[{{"price":"0.50","size":"100"}}]}}"#
        ))
        .await;
}

/// Helper: send a Chainlink reference price.
async fn send_chainlink_price(chainlink_mock: &MockWsServer) {
    chainlink_mock
        .send(&format!(
            r#"{{"topic":"crypto_prices_chainlink","payload":{{"value":42000,"timestamp":{}}}}}"#,
            current_test_ms(),
        ))
        .await;
}

/// Helper: compute wall-clock timing values for a test window.
/// `offset_secs` controls how many seconds from now the window ends.
fn compute_window_timing_with_offset(offset_secs: u64) -> (u64, u64, String) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let end_time_secs = now_secs + offset_secs;
    #[allow(clippy::cast_possible_wrap)]
    let end_date = chrono::DateTime::from_timestamp(end_time_secs as i64, 0)
        .unwrap()
        .to_rfc3339();
    let current_slot = (now_secs / 300) * 300;
    (now_secs, current_slot, end_date)
}

/// Current test ms.
fn current_test_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Helper: run the bot, send scripted ticks, wait for window close, and shut down.
/// Returns the `run_live` result so the caller can check for errors.
/// The `window_secs` parameter controls how long to wait for the window to close.
async fn run_one_window(
    config: Config,
    db_path: &str,
    balance: f64,
    binance_mock: &MockWsServer,
    clob_mock: &MockWsServer,
    chainlink_mock: &MockWsServer,
    window_secs: u64,
) -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_owned = db_path.to_string();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_owned, balance, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(binance_mock, 25).await;
    send_chainlink_price(chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(window_secs + 1)).await;

    let settle_deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        let settled_count = {
            let conn = rusqlite::Connection::open(db_path).unwrap();
            conn.query_row("SELECT COUNT(*) FROM trade_results", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0)
        };
        if settled_count > 0 || tokio::time::Instant::now() >= settle_deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    inner.unwrap()
}

/// End-to-end system test: boots the full live bot with mock servers, feeds it
/// scripted messages for one market window, then verifies the database state.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_processes_one_market_window() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let end_time_secs = now_secs + 4;
    #[allow(clippy::cast_possible_wrap)]
    let end_date = chrono::DateTime::from_timestamp(end_time_secs as i64, 0)
        .unwrap()
        .to_rfc3339();

    let current_slot = (now_secs / 300) * 300;

    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{current_slot}"),
            "markets": [{
                "id": "mkt-sys-test",
                "question": "Will BTC go up?",
                "conditionId": "cond-sys",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-sys", "tok-down-sys"],
                "endDate": end_date
            }]
        })))
        .mount(&gamma_mock)
        .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let config = Config {
        binance_ws_url: binance_mock.url.clone(),
        clob_ws_url: clob_mock.url.clone(),
        rtds_ws_url: chainlink_mock.url.clone(),
        gamma_api_url: gamma_mock.uri(),
        gamma_poll_interval: 500,
        tick_interval: 200,
        clob_ping_interval: 30_000,
        rtds_ping_interval: 30_000,
        chainlink_stale_ms: 10_000,
        reconnect_base_delay: 100,
        reconnect_max_delay: 500,
        latency_arb_momentum_threshold: 0.001,
        latency_arb_max_ask: 0.60,
        latency_arb_min_ask: 0.20,
        latency_arb_cooldown_ms: 100,
        min_window_time_ms: 1_000,
        starting_balance: 200.0,
        peak_dd_pause_pct: 1.0,
        spread_capture_threshold: 0.50,
        momentum_window_ms: 5_000,
        log_level: "warn".to_string(),
        resolution_poll_retries: 0,
        resolution_initial_delay_ms: 0,
        resolution_poll_delay_ms: 0,
        ..Config::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let base_price = 42_000.0;
    for i in 0_u32..20 {
        let price = base_price + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    chainlink_mock
        .send(&format!(
            r#"{{"topic":"crypto_prices_chainlink","payload":{{"value":42000,"timestamp":{}}}}}"#,
            current_test_ms(),
        ))
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let timestamp = current_test_ms();
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-up-sys","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.45","size":"100"}}]}}"#
        ))
        .await;
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-down-sys","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.50","size":"100"}}]}}"#
        ))
        .await;

    tokio::time::sleep(Duration::from_secs(5)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );

    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let market_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert!(
        market_count > 0,
        "Expected at least 1 market, got {market_count}"
    );

    let balance_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM balance_log", [], |r| r.get(0))
        .unwrap();
    assert!(
        balance_count > 0,
        "Expected balance_log entries, got {balance_count}"
    );

    let feed_event_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM feed_events", [], |r| r.get(0))
        .unwrap();
    assert!(
        feed_event_count > 0,
        "Expected feed_events entries, got {feed_event_count}"
    );
}

/// A1: Verify that the live bot actually executes trades, settles them on window
/// close, and records results in the database.  The existing test only checks
/// for market/balance\_log/feed\_events existence; this one verifies the full
/// trade lifecycle: open -> close -> `trade_results`.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settles_trades_and_records_results() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    register_gamma_mock_with_resolution(&gamma_mock, current_slot, &end_date, "UP").await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;

    send_chainlink_price(&chainlink_mock).await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let closed_trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE status = 'closed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        closed_trade_count >= 1,
        "Expected at least 1 closed trade, got {closed_trade_count}"
    );

    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert!(
        result_count >= 1,
        "Expected at least 1 trade result, got {result_count}"
    );

    let final_balance: f64 = conn
        .query_row(
            "SELECT balance FROM balance_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        (final_balance - 200.0).abs() > f64::EPSILON,
        "Expected balance to differ from 200.0 after trade settlement, got {final_balance}"
    );

    let pnl: f64 = conn
        .query_row("SELECT pnl_0pct FROM trade_results LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();

    assert!(pnl.abs() > f64::EPSILON, "Expected non-zero PnL, got {pnl}");

    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        open_count, 0,
        "Expected 0 open trades after window close, got {open_count}"
    );
}

/// A2: Verify that when the Chainlink feed goes stale, the bot falls back to
/// Binance price for settlement and continues to function correctly.
/// Tests live.rs lines 232-234 (`ChainlinkStale` handling) and line 284
/// (`close_price` falls back to Binance when `chainlink_price` is `None`).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_chainlink_stale_uses_binance_fallback() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    register_gamma_mock_with_resolution(&gamma_mock, current_slot, &end_date, "UP").await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.chainlink_stale_ms = 500;
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(1500)).await;

    send_chainlink_price(&chainlink_mock).await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    send_rising_binance_ticks(&binance_mock, 25).await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    let run_result = inner.unwrap();
    assert!(
        run_result.is_ok(),
        "run_live returned an error (bot crashed): {run_result:?}"
    );

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert!(
        result_count >= 1,
        "Expected at least 1 trade result even with stale Chainlink, got {result_count}"
    );

    let settlement: f64 = conn
        .query_row(
            "SELECT settlement_price FROM trade_results LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        (settlement - 0.0).abs() < f64::EPSILON || (settlement - 1.0).abs() < f64::EPSILON,
        "Expected binary settlement (0.0 or 1.0), got {settlement}"
    );

    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        open_count, 0,
        "Expected 0 open trades after settlement, got {open_count}"
    );
}

/// A3: Verify that when the bot restarts with the same DB path, it recovers
/// its balance from the database instead of using the CLI-specified starting
/// balance.  This tests `BankrollManager::new()`'s balance recovery logic.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_recovers_balance_from_db() {
    let binance_mock_1 = MockWsServer::start().await;
    let clob_mock_1 = MockWsServer::start().await;
    let chainlink_mock_1 = MockWsServer::start().await;
    let gamma_mock_1 = MockServer::start().await;

    let (_now_secs_1, current_slot_1, end_date_1) = compute_window_timing_with_offset(8);
    register_gamma_mock_with_resolution(&gamma_mock_1, current_slot_1, &end_date_1, "UP").await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config_1 = test_config(
        &binance_mock_1.url,
        &clob_mock_1.url,
        &chainlink_mock_1.url,
        &gamma_mock_1.uri(),
    );
    config_1.min_window_time_ms = 0;

    let result_1 = run_one_window(
        config_1,
        &db_path,
        200.0,
        &binance_mock_1,
        &clob_mock_1,
        &chainlink_mock_1,
        8,
    )
    .await;
    assert!(result_1.is_ok(), "first run failed: {result_1:?}");

    let first_run_final_balance: f64 = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT balance FROM balance_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };

    let first_run_trade_count: i64 = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
            .unwrap()
    };

    let binance_mock_2 = MockWsServer::start().await;
    let clob_mock_2 = MockWsServer::start().await;
    let chainlink_mock_2 = MockWsServer::start().await;
    let gamma_mock_2 = MockServer::start().await;

    let (_now_secs_2, current_slot_2, end_date_2) = compute_window_timing_with_offset(6);
    register_gamma_mock_with_resolution(&gamma_mock_2, current_slot_2, &end_date_2, "UP").await;

    let config_2 = test_config(
        &binance_mock_2.url,
        &clob_mock_2.url,
        &chainlink_mock_2.url,
        &gamma_mock_2.uri(),
    );

    #[allow(clippy::similar_names)]
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    let db_path2 = db_path.clone();
    let bot_handle_2 = tokio::spawn(async move {
        buba_paint::live::run_live(config_2, &db_path2, 999.0, stop_receiver).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock_2, 20).await;
    send_chainlink_price(&chainlink_mock_2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock_2).await;

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = stop_sender.send(());
    let result_2 = tokio::time::timeout(Duration::from_secs(5), bot_handle_2).await;
    assert!(
        result_2.is_ok(),
        "second bot did not shut down within timeout"
    );
    let inner_2 = result_2.unwrap();
    assert!(inner_2.is_ok(), "second bot panicked: {inner_2:?}");
    assert!(
        inner_2.unwrap().is_ok(),
        "second run_live returned an error"
    );

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let mut stmt = conn
        .prepare("SELECT event, balance FROM balance_log ORDER BY id ASC")
        .unwrap();
    let entries: Vec<(String, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(
        entries.len() >= 2,
        "Expected at least 2 balance_log entries, got {}",
        entries.len()
    );

    assert!(
        (entries[0].1 - 200.0).abs() < f64::EPSILON,
        "First init balance should be 200.0, got {}",
        entries[0].1
    );

    let has_999 = entries
        .iter()
        .any(|(_, bal)| (*bal - 999.0).abs() < f64::EPSILON);
    assert!(
        !has_999,
        "Balance 999.0 should NOT appear in balance_log — \
         the bot should have recovered from DB. Entries: {entries:?}"
    );

    if first_run_trade_count > 0 {
        let second_run_balances: Vec<f64> = entries.iter().skip(1).map(|(_, bal)| *bal).collect();
        for bal in &second_run_balances {
            assert!(
                (*bal - 999.0).abs() > 0.01,
                "Balance {bal} is suspiciously close to 999.0 — \
                 recovery may have failed. First run final: {first_run_final_balance}"
            );
        }
    }
}

/// A4: Verify that the bot handles multiple market windows in sequence,
/// resetting state between them and recording distinct markets in the DB.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_multiple_windows_in_sequence() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_1 = now_secs + 6;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_1 = chrono::DateTime::from_timestamp(end_time_1 as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_2 = now_secs + 12;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_2 = chrono::DateTime::from_timestamp(end_time_2 as i64, 0)
        .unwrap()
        .to_rfc3339();

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{current_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{current_slot}"),
            "markets": [{
                "id": "mkt-window-1",
                "question": "Will BTC go up? (window 1)",
                "conditionId": "cond-w1",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-w1", "tok-down-w1"],
                "endDate": end_date_1
            }]
        })))
        .mount(&gamma_mock)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{next_slot}"),
            "markets": [{
                "id": "mkt-window-2",
                "question": "Will BTC go up? (window 2)",
                "conditionId": "cond-w2",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-w2", "tok-down-w2"],
                "endDate": end_date_2
            }]
        })))
        .mount(&gamma_mock)
        .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 20).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_secs(6)).await;

    send_rising_binance_ticks(&binance_mock, 20).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let market_count: i64 = conn
        .query_row("SELECT COUNT(DISTINCT market_id) FROM markets", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        market_count >= 2,
        "Expected at least 2 distinct market_ids, got {market_count}"
    );

    let has_w1: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM markets WHERE market_id = 'mkt-window-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(has_w1, "Expected mkt-window-1 in markets table");

    let has_w2: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM markets WHERE market_id = 'mkt-window-2'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(has_w2, "Expected mkt-window-2 in markets table");
}

/// Verifies that a no-signal live window persists aggregated rejection summaries.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_persists_rejection_summaries_for_no_signal_window() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.spread_capture_enabled = true;
    config.spread_capture_threshold = 0.97;
    config.latency_arb_max_ask = 0.60;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_flat_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_high_ask_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let summary_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM strategy_rejection_summaries",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        summary_count > 0,
        "Expected rejection summaries for a no-signal window, got {summary_count}"
    );

    let spread_threshold_count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM strategy_rejection_summaries
             WHERE strategy = 'spread-capture' AND reason = 'spread_threshold_not_met'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        spread_threshold_count > 0,
        "Expected spread_threshold_not_met summaries, got {spread_threshold_count}"
    );
}

/// Verifies that impossible spread setups are rejected before signal persistence.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_rejects_unaffordable_spread_before_logging_signal_rows() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.spread_capture_enabled = true;
    config.spread_capture_threshold = 1.0;
    config.spread_capture_max_quote_churn_per_s = 50.0;
    config.max_position_fraction = 0.05;
    config.spread_capture_max_position_fraction = Some(0.05);
    config.max_position_usd_fraction = 1.0;
    config.max_position_usd = 1_000.0;
    config.latency_arb_max_ask = 0.60;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_flat_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_unaffordable_spread_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_secs(4)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let signal_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM signals WHERE strategy = 'spread-capture'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        signal_count, 0,
        "Expected no persisted spread signals for impossible pre-queue setups, got {signal_count}"
    );

    let metric_count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM signal_metrics
             WHERE signal_id IN (
                 SELECT id FROM signals WHERE strategy = 'spread-capture'
             )",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        metric_count, 0,
        "Expected no spread signal_metrics rows for impossible pre-queue setups, got {metric_count}"
    );

    let rejection_count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM strategy_rejection_summaries
             WHERE strategy = 'spread-capture' AND reason = 'spread_budget_too_small'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        rejection_count > 0,
        "Expected spread_budget_too_small rejection summaries, got {rejection_count}"
    );

    let details_json: String = conn
        .query_row(
            "SELECT details_json
             FROM strategy_rejection_summaries
             WHERE strategy = 'spread-capture' AND reason = 'spread_budget_too_small'
             ORDER BY id DESC
             LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let details: serde_json::Value = serde_json::from_str(&details_json).unwrap();
    let available = details["last"]["availableSpreadBudget"].as_f64().unwrap();
    let required = details["last"]["requiredPairNotional"].as_f64().unwrap();
    let units = details["last"]["requiredPairUnits"].as_f64().unwrap();
    assert!(
        (available - 10.0).abs() < 1e-9,
        "expected available spread budget near 10.0, got {available}"
    );
    assert!(
        required > available,
        "expected required notional to exceed available budget, got required={required} available={available}"
    );
    assert!(
        (units - 11.0).abs() < 1e-9,
        "expected 11 pair units to satisfy the per-leg minimum, got {units}"
    );
}

/// Verifies that missing `CLOB` source timestamps no longer poison live quote
/// freshness and still allow latency-arb to generate signals.
#[tokio::test]
async fn live_bot_zero_timestamp_clob_books_still_generate_signal() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.spread_capture_threshold = 0.97;
    config.latency_arb_momentum_threshold = 0.0005;
    config.latency_arb_max_ask = 0.60;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book_without_timestamp(&clob_mock).await;
    send_rising_binance_ticks(&binance_mock, 10).await;

    tokio::time::sleep(Duration::from_secs(4)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let signal_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM signals", [], |r| r.get(0))
        .unwrap();
    assert!(
        signal_count > 0,
        "Expected at least one generated signal with zero-timestamp CLOB books, got {signal_count}"
    );

    let stale_count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM strategy_rejection_summaries
             WHERE strategy = 'latency-arb' AND reason = 'features_stale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stale_count, 0,
        "Expected zero latency-arb features_stale summaries after the observed-freshness fix, got {stale_count}"
    );
}

/// Verifies that direct best-bid-ask updates without size fields preserve
/// liquidity and allow queued paper orders to fill.
#[tokio::test]
async fn live_bot_best_bid_ask_without_sizes_still_opens_trade() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.spread_capture_threshold = 0.97;
    config.latency_arb_momentum_threshold = 0.0005;
    config.latency_arb_max_ask = 0.60;
    config.sim_order_latency_ms = 100;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;
    wait_for_persisted_clob_snapshot_with_sizes(&db_path).await;
    send_rising_binance_ticks(&binance_mock, 6).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    send_clob_best_bid_ask_without_sizes(&clob_mock).await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let trade_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM simulated_trades", [], |r| r.get(0))
        .unwrap();
    assert!(
        trade_count > 0,
        "Expected at least one trade after direct best_bid_ask without sizes, got {trade_count}"
    );

    let filled_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM signal_metrics WHERE decision_status = 'filled'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        filled_count > 0,
        "Expected at least one filled signal metric, got {filled_count}"
    );

    let legacy_best_bid_ask_with_size: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM feed_events
             WHERE event_type = 'best_bid_ask'
               AND source IN ('clob_up', 'clob_down')
               AND ask_size > 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let compact_best_bid_ask_with_size: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM clob_replay_events
             WHERE event_type = 'best_bid_ask'
               AND source IN ('clob_up', 'clob_down')
               AND ask_size > 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        legacy_best_bid_ask_with_size + compact_best_bid_ask_with_size > 0,
        "Expected persisted best_bid_ask rows to preserve positive ask_size, got {}",
        legacy_best_bid_ask_with_size + compact_best_bid_ask_with_size
    );
}

/// A5: Verify that the `StrategyResult::Batch` path (spread-capture strategy)
/// works end-to-end in the live bot: both legs of the spread are opened.
/// Covers live.rs lines 447-468 (Batch branch in `evaluate_strategies`).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_spread_capture_executes() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.spread_capture_enabled = true;
    config.spread_capture_max_position_fraction = Some(0.60);
    config.spread_capture_threshold = 0.90;
    config.spread_capture_min_ask = 0.15;
    config.max_position_usd_fraction = 1.0;
    config.latency_arb_momentum_threshold = 99.0;
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 10).await;

    send_chainlink_price(&chainlink_mock).await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let timestamp = current_test_ms();
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-up-sys","timestamp":{timestamp},"bids":[{{"price":"0.35","size":"100"}}],"asks":[{{"price":"0.40","size":"100"}}]}}"#
        ))
        .await;
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-down-sys","timestamp":{timestamp},"bids":[{{"price":"0.35","size":"100"}}],"asks":[{{"price":"0.40","size":"100"}}]}}"#
        ))
        .await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 10_u32..20 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let spread_trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE strategy = 'spread-capture'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        spread_trade_count >= 2,
        "Expected at least 2 spread-capture trades (both legs), got {spread_trade_count}"
    );

    let up_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE strategy = 'spread-capture' AND side = 'UP'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let down_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE strategy = 'spread-capture' AND side = 'DOWN'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        up_count >= 1,
        "Expected at least 1 spread-capture UP trade, got {up_count}"
    );
    assert!(
        down_count >= 1,
        "Expected at least 1 spread-capture DOWN trade, got {down_count}"
    );

    let latency_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE strategy = 'latency-arb'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        latency_count, 0,
        "Expected 0 latency-arb trades (disabled), got {latency_count}"
    );
}

/// A6: Verify that the circuit breaker prevents trading after consecutive losses.
/// Both windows run within a SINGLE bot instance so the in-memory CB state
/// carries over from window 1 (loss) to window 2 (blocked).
/// Covers live.rs lines 396-398 (CB check in `evaluate_strategies`).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_circuit_breaker_blocks_trading() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_1 = now_secs + 12;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_1 = chrono::DateTime::from_timestamp(end_time_1 as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_2 = now_secs + 24;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_2 = chrono::DateTime::from_timestamp(end_time_2 as i64, 0)
        .unwrap()
        .to_rfc3339();

    register_gamma_mock_with_resolution(&gamma_mock, current_slot, &end_date_1, "DOWN").await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.circuit_breaker_losses = 1;
    config.circuit_breaker_pause_ms = 120_000;
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    send_rising_binance_ticks(&binance_mock, 10).await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let base_low = 40_000.0;
    for i in 0_u32..15 {
        let price = base_low - f64::from(i) * 10.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + 5000 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + 5000 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(12)).await;

    gamma_mock.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{next_slot}"),
            "markets": [{
                "id": "mkt-cb-w2",
                "question": "Will BTC go up? (CB window 2)",
                "conditionId": "cond-cb-w2",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-sys", "tok-down-sys"],
                "endDate": end_date_2
            }]
        })))
        .mount(&gamma_mock)
        .await;

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    send_rising_binance_ticks(&binance_mock, 15).await;

    tokio::time::sleep(Duration::from_secs(14)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let w1_trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-sys-test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        w1_trade_count >= 1,
        "Expected at least 1 trade in window 1, got {w1_trade_count}"
    );

    let w2_trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-cb-w2'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        w2_trade_count, 0,
        "Expected 0 trades in window 2 (CB should block), got {w2_trade_count}"
    );
}

/// TS1: Verify that a rising-price window produces an UP trade that settles
/// at `settlement_price` = 1.0 with positive `PnL`.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settles_up_trade_with_correct_settlement() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    register_gamma_mock_with_resolution(&gamma_mock, current_slot, &end_date, "UP").await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let result = run_one_window(
        config,
        &db_path,
        200.0,
        &binance_mock,
        &clob_mock,
        &chainlink_mock,
        8,
    )
    .await;
    assert!(result.is_ok(), "run_live failed: {result:?}");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let side: String = conn
        .query_row(
            "SELECT side FROM simulated_trades WHERE strategy = 'latency-arb' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        side, "UP",
        "Expected UP trade from rising prices, got {side}"
    );

    let settlement: f64 = conn
        .query_row(
            "SELECT settlement_price FROM trade_results LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        (settlement - 1.0).abs() < f64::EPSILON,
        "Expected settlement_price=1.0 for UP win, got {settlement}"
    );

    let pnl: f64 = conn
        .query_row("SELECT pnl_0pct FROM trade_results LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(pnl > 0.0, "Expected positive PnL for UP win, got {pnl}");
}

/// TS2: Verify that a falling-price window produces a DOWN trade that settles
/// at `settlement_price` = 1.0 with positive `PnL`.
/// High initial prices are captured as `open_price`, then falling prices produce
/// negative momentum for a DOWN signal. At close, latest price < `open_price`
/// so DOWN wins.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settles_down_trade_with_correct_settlement() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    mount_gamma_sequence(
        &gamma_mock,
        r"^/events/slug/btc-updown-5m-\d+$".to_string(),
        gamma_market_response(
            format!("btc-updown-5m-{current_slot}"),
            "mkt-ts2-down",
            "cond-ts2",
            "tok-up-ts2",
            "tok-down-ts2",
            end_date.as_str(),
            None,
        ),
        gamma_market_response(
            format!("btc-updown-5m-{current_slot}"),
            "mkt-ts2-down",
            "cond-ts2",
            "tok-up-ts2",
            "tok-down-ts2",
            end_date.as_str(),
            Some("DOWN"),
        ),
    )
    .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    config.latency_arb_momentum_threshold = 0.0005;

    config.latency_arb_max_ask = 0.65;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    for i in 0_u32..10 {
        let price = 42_000.0 + f64::from(i);
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let timestamp = current_test_ms();
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-up-ts2","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.50","size":"100"}}]}}"#
        ))
        .await;
    clob_mock
        .send(&format!(
            r#"{{"asset_id":"tok-down-ts2","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.45","size":"100"}}]}}"#
        ))
        .await;

    tokio::time::sleep(Duration::from_millis(300)).await;

    for i in 0_u32..25 {
        let price = 40_000.0 - f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_001_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_001_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE side = 'DOWN'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        trade_count >= 1,
        "Expected at least 1 DOWN trade, got {trade_count}"
    );

    let settlement: f64 = conn
        .query_row(
            "SELECT tr.settlement_price FROM trade_results tr \
             JOIN simulated_trades t ON tr.trade_id = t.id \
             WHERE t.side = 'DOWN' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        (settlement - 1.0).abs() < f64::EPSILON,
        "Expected settlement_price=1.0 for DOWN win, got {settlement}"
    );

    let pnl: f64 = conn
        .query_row(
            "SELECT tr.pnl_0pct FROM trade_results tr \
             JOIN simulated_trades t ON tr.trade_id = t.id \
             WHERE t.side = 'DOWN' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(pnl > 0.0, "Expected positive PnL for DOWN win, got {pnl}");
}

/// TS3: Verify that settlement `PnL` exactly matches the expected calculation:
/// `pnl_0pct` = (`settlement_price` - `entry_price`) * size.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settlement_pnl_matches_expected_calculation() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    register_gamma_mock_with_resolution(&gamma_mock, current_slot, &end_date, "UP").await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let result = run_one_window(
        config,
        &db_path,
        200.0,
        &binance_mock,
        &clob_mock,
        &chainlink_mock,
        8,
    )
    .await;
    assert!(result.is_ok(), "run_live failed: {result:?}");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let (entry_price, size): (f64, f64) = conn
        .query_row(
            "SELECT entry_price, size FROM simulated_trades WHERE status = 'closed' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    let (actual_pnl, settlement): (f64, f64) = conn
        .query_row(
            "SELECT pnl_0pct, settlement_price FROM trade_results LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    let expected_pnl = (settlement - entry_price) * size;
    assert!(
        (actual_pnl - expected_pnl).abs() < 0.01,
        "PnL mismatch: actual={actual_pnl}, expected={expected_pnl} \
         (settlement={settlement}, entry={entry_price}, size={size})"
    );
}

/// SC1: Verify that book state resets between windows. Window A has `CLOB` data
/// and trades; window B has NO `CLOB` data sent for its tokens, so the stale
/// book from A must not leak through.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_book_state_resets_between_windows() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_a = now_secs + 12;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_a = chrono::DateTime::from_timestamp(end_time_a as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_b = now_secs + 24;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    mount_gamma_sequence(
        &gamma_mock,
        format!("^/events/slug/btc-updown-5m-{current_slot}$"),
        gamma_market_response(
            format!("btc-updown-5m-{current_slot}"),
            "mkt-sys-test",
            "cond-sys",
            "tok-up-sys",
            "tok-down-sys",
            end_date_a.as_str(),
            None,
        ),
        gamma_market_response(
            format!("btc-updown-5m-{current_slot}"),
            "mkt-sys-test",
            "cond-sys",
            "tok-up-sys",
            "tok-down-sys",
            end_date_a.as_str(),
            Some("DOWN"),
        ),
    )
    .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(12)).await;

    gamma_mock.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{next_slot}"),
            "markets": [{
                "id": "mkt-sc1-b",
                "question": "Will BTC go up? (B)",
                "conditionId": "cond-sc1-b",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-sc1b", "tok-down-sc1b"],
                "endDate": end_date_b
            }]
        })))
        .mount(&gamma_mock)
        .await;

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    send_rising_binance_ticks(&binance_mock, 15).await;

    tokio::time::sleep(Duration::from_secs(14)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let a_trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-sys-test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        a_trade_count >= 1,
        "Expected trades in window A, got {a_trade_count}"
    );

    let b_trade_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-sc1-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        b_trade_count, 0,
        "Expected 0 trades in window B (book should have reset), got {b_trade_count}"
    );
}

/// ER1: Verify that no trades are placed when Binance feed sends no data.
/// The `evaluate_strategies` function requires `binance_price.is_some()`.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_no_trade_when_no_binance_price() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(6);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_secs(8)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let trade_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM simulated_trades", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        trade_count, 0,
        "Expected 0 trades when no Binance price, got {trade_count}"
    );

    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        result_count, 0,
        "Expected 0 trade results when no Binance price, got {result_count}"
    );
}

/// BI1: Verify that the drawdown pause blocks trading after a losing trade.
/// Window A: UP trade loses (close < open). Loss triggers DD pause
/// (`peak_dd_pause_pct` set very low). Window B: all data present but DD pause
/// blocks `can_trade()` so 0 trades.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_dd_pause_blocks_trading_in_live() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_a = now_secs + 10;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_a = chrono::DateTime::from_timestamp(end_time_a as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_b = now_secs + 40;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    let window_a_unresolved = gamma_market_response(
        format!("btc-updown-5m-{current_slot}"),
        "mkt-sys-test",
        "cond-sys",
        "tok-up-sys",
        "tok-down-sys",
        &end_date_a,
        None,
    );
    let window_a_resolved = gamma_market_response(
        format!("btc-updown-5m-{current_slot}"),
        "mkt-sys-test",
        "cond-sys",
        "tok-up-sys",
        "tok-down-sys",
        &end_date_a,
        Some("DOWN"),
    );
    mount_gamma_sequence(
        &gamma_mock,
        format!("^/events/slug/btc-updown-5m-{current_slot}$"),
        window_a_unresolved,
        window_a_resolved,
    )
    .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.peak_dd_pause_pct = 0.01;
    config.peak_dd_pause_ms = 120_000;
    config.circuit_breaker_losses = 999;
    config.circuit_breaker_pause_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    for i in 0_u32..15 {
        let price = 39_000.0 - f64::from(i) * 10.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_005_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_005_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(10)).await;

    gamma_mock.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{next_slot}"),
            "markets": [{
                "id": "mkt-bi1-b",
                "question": "Will BTC go up? (B)",
                "conditionId": "cond-bi1-b",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-sys", "tok-down-sys"],
                "endDate": end_date_b
            }]
        })))
        .mount(&gamma_mock)
        .await;

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 35_u32..50 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(12)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let a_trades: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-sys-test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        a_trades >= 1,
        "Expected at least 1 trade in window A, got {a_trades}"
    );

    let b_trades: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-bi1-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        b_trades, 0,
        "Expected 0 trades in window B (DD pause should block), got {b_trades}"
    );
}

/// BI2: Verify that the circuit breaker recovers within the same bot run.
/// Window A loses and trips the breaker, then window B remains open long enough
/// for discovery, pause expiry, and a fresh post-recovery trade.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_circuit_breaker_recovers_within_same_run() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_a = now_secs + 8;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_a = chrono::DateTime::from_timestamp(end_time_a as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_b = now_secs + 36;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{current_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{current_slot}"),
            "markets": [{
                "id": "mkt-sys-test",
                "question": "Will BTC go up?",
                "conditionId": "cond-sys",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-sys", "tok-down-sys"],
                "endDate": end_date_a
            }]
        })))
        .mount(&gamma_mock)
        .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.circuit_breaker_losses = 1;
    config.circuit_breaker_pause_ms = 6_000;
    config.latency_arb_adaptive_window_ms = 5_000;
    config.peak_dd_pause_pct = 1.0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    for i in 0_u32..15 {
        let price = 39_000.0 - f64::from(i) * 10.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_005_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_005_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(8)).await;
    tokio::time::sleep(Duration::from_secs(8)).await;

    gamma_mock.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{next_slot}"),
            "markets": [{
                "id": "mkt-bi2-b",
                "question": "Will BTC go up? (B)",
                "conditionId": "cond-bi2-b",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-sys", "tok-down-sys"],
                "endDate": end_date_b
            }]
        })))
        .mount(&gamma_mock)
        .await;

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let second_window_base_ts = 1_700_000_010_000_u64;
    for i in 0_u32..25 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let ts = second_window_base_ts + u64::from(i) * 100;
        let msg = format!(r#"{{"e":"aggTrade","E":{ts},"p":"{price}","q":"0.01","T":{ts}}}"#);
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 35_u32..50 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let ts = second_window_base_ts + 3_000 + u64::from(i - 35) * 100;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            ts, price, ts,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(6)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let a_trades: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-sys-test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        a_trades >= 1,
        "Expected at least 1 trade in window A, got {a_trades}"
    );

    let b_trades: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-bi2-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        b_trades >= 1,
        "Expected at least 1 trade in window B after CB recovery, got {b_trades}"
    );
}

/// A7: Verify that the bot survives concurrent feed disconnections without
/// panicking and returns `Ok` on shutdown.
/// Covers live.rs lines 228-230 (`FeedDisconnected` handling).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_survives_concurrent_feed_disconnections() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(30);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_secs(2)).await;

    binance_mock.close().await;
    clob_mock.close().await;
    chainlink_mock.close().await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout -- it may have panicked or hung"
    );
    let inner = result.unwrap();
    assert!(
        inner.is_ok(),
        "bot task panicked after feed disconnections: {inner:?}"
    );
    let run_result = inner.unwrap();
    assert!(
        run_result.is_ok(),
        "run_live returned an error after feed disconnections: {run_result:?}"
    );
}

/// Verifies that idle reconnects still leave stale-data trading safely blocked.
#[tokio::test]
async fn live_bot_idle_reconnects_do_not_bypass_freshness_gating() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(12);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.binance_no_message_reconnect_ms = 300;
    config.clob_no_message_reconnect_ms = 300;
    config.reconnect_base_delay = 50;
    config.reconnect_max_delay = 50;
    config.chainlink_stale_ms = 5_000;
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(1_500)).await;
    send_rising_binance_ticks(&binance_mock, 10).await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(result.is_ok(), "bot did not shut down within timeout");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let trade_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM simulated_trades", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        trade_count, 0,
        "Expected no trades while feeds were repeatedly going stale, got {trade_count}"
    );

    let filled_signal_metrics: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM signal_metrics
             WHERE decision_status = 'filled'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        filled_signal_metrics, 0,
        "Expected zero filled signal metrics while feeds were idling and reconnecting"
    );

    let binance_idle_disconnects: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM feed_health_events
             WHERE source = 'binance'
               AND event_type = 'disconnected'
               AND json_extract(details_json, '$.causeClass') = 'idle_timeout'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        binance_idle_disconnects > 0,
        "Expected at least one Binance idle-timeout disconnect"
    );

    let clob_idle_disconnects: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM feed_health_events
             WHERE source = 'clob'
               AND event_type = 'disconnected'
               AND json_extract(details_json, '$.causeClass') = 'idle_timeout'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        clob_idle_disconnects > 0,
        "Expected at least one CLOB idle-timeout disconnect"
    );
}

/// Helper: send a `CLOB` book snapshot for custom token IDs.
///
/// Sends both UP and DOWN snapshots as a `JSON` array so the `CLOB` feed builds
/// a full `BookState` (with both sides) in a single `WebSocket` frame.  This
/// avoids the 200ms eval throttle eating the first `ClobBook` event (up-only)
/// and leaving the second (up+down) throttled.
async fn send_clob_book_for_tokens(clob_mock: &MockWsServer, up_token: &str, down_token: &str) {
    clob_mock
        .send(&format!(
            r#"[{{"asset_id":"{up_token}","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.45","size":"100"}}]}},{{"asset_id":"{down_token}","timestamp":{timestamp},"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.50","size":"100"}}]}}]"#,
            timestamp = current_test_ms(),
        ))
        .await;
}

/// WL1: THE REGRESSION TEST — trades must be settled even when market discovery
/// has already advanced `current_window` to the next slot before `WindowClosed`
/// fires for the previous slot.
///
/// Without the `known_windows` fix this test fails: window A's trades remain
/// open because `WindowClosed(A)` fires after `current_window` is already B,
/// and the old code only checked `current_window`.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settles_trades_when_next_window_already_discovered() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_a = now_secs + 15;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_a = chrono::DateTime::from_timestamp(end_time_a as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_b = now_secs + 30;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    mount_gamma_sequence(
        &gamma_mock,
        format!("^/events/slug/btc-updown-5m-{current_slot}$"),
        gamma_market_response(
            format!("btc-updown-5m-{current_slot}"),
            "mkt-wl1-a",
            "cond-wl1-a",
            "tok-up-wl1a",
            "tok-down-wl1a",
            end_date_a.as_str(),
            None,
        ),
        gamma_market_response(
            format!("btc-updown-5m-{current_slot}"),
            "mkt-wl1-a",
            "cond-wl1-a",
            "tok-up-wl1a",
            "tok-down-wl1a",
            end_date_a.as_str(),
            Some("UP"),
        ),
    )
    .await;

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "slug": format!("btc-updown-5m-{next_slot}"),
                    "markets": [{
                        "id": "mkt-wl1-b",
                        "question": "Will BTC go up? (WL1 window B)",
                        "conditionId": "cond-wl1-b",
                        "outcomes": ["Up", "Down"],
                        "clobTokenIds": ["tok-up-wl1b", "tok-down-wl1b"],
                        "endDate": end_date_b
                    }]
                }))
                .set_delay(Duration::from_secs(8)),
        )
        .mount(&gamma_mock)
        .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.latency_arb_momentum_threshold = 0.001;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book_for_tokens(&clob_mock, "tok-up-wl1a", "tok-down-wl1a").await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(15)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let closed_a: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-wl1-a' AND status = 'closed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        closed_a >= 1,
        "Expected at least 1 closed trade for window A (mkt-wl1-a), got {closed_a}. \
         This is the regression: without the known_windows fix, trades are never settled \
         when current_window has already advanced to the next window."
    );

    let results_a: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM trade_results r \
             JOIN simulated_trades t ON r.trade_id = t.id \
             WHERE t.market_id = 'mkt-wl1-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        results_a >= 1,
        "Expected at least 1 trade_result for window A, got {results_a}"
    );

    let final_balance: f64 = conn
        .query_row(
            "SELECT balance FROM balance_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        (final_balance - 200.0).abs() > f64::EPSILON,
        "Expected balance to differ from 200.0 after settlement, got {final_balance}"
    );

    let open_a: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-wl1-a' AND status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        open_a, 0,
        "Expected 0 open trades for window A after WindowClosed, got {open_a}"
    );
}

/// WL2: Two windows are discovered simultaneously; trades open in B (the later
/// window that becomes `current_window`) and must be settled when B's
/// `WindowClosed` fires, while A closes gracefully with zero trades.
/// Verifies that `known_windows` tracks both windows and settles each correctly.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settles_both_overlapping_windows() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let current_slot = (now_secs / 300) * 300;
    let next_slot = current_slot + 300;

    let end_time_a = now_secs + 8;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_a = chrono::DateTime::from_timestamp(end_time_a as i64, 0)
        .unwrap()
        .to_rfc3339();

    let end_time_b = now_secs + 18;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{current_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{current_slot}"),
            "markets": [{
                "id": "mkt-wl2-a",
                "question": "Will BTC go up? (WL2 window A)",
                "conditionId": "cond-wl2-a",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-wl2a", "tok-down-wl2a"],
                "endDate": end_date_a
            }]
        })))
        .mount(&gamma_mock)
        .await;

    mount_gamma_sequence(
        &gamma_mock,
        format!("^/events/slug/btc-updown-5m-{next_slot}$"),
        gamma_market_response(
            format!("btc-updown-5m-{next_slot}"),
            "mkt-wl2-b",
            "cond-wl2-b",
            "tok-up-wl2b",
            "tok-down-wl2b",
            end_date_b.as_str(),
            None,
        ),
        gamma_market_response(
            format!("btc-updown-5m-{next_slot}"),
            "mkt-wl2-b",
            "cond-wl2-b",
            "tok-up-wl2b",
            "tok-down-wl2b",
            end_date_b.as_str(),
            Some("UP"),
        ),
    )
    .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.latency_arb_momentum_threshold = 0.001;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book_for_tokens(&clob_mock, "tok-up-wl2b", "tok-down-wl2b").await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(16)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let market_count: i64 = conn
        .query_row("SELECT COUNT(DISTINCT market_id) FROM markets", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        market_count >= 2,
        "Expected at least 2 markets, got {market_count}"
    );

    let closed_b: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-wl2-b' AND status = 'closed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        closed_b >= 1,
        "Expected at least 1 closed trade for window B (mkt-wl2-b), got {closed_b}"
    );

    let results_b: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM trade_results r \
             JOIN simulated_trades t ON r.trade_id = t.id \
             WHERE t.market_id = 'mkt-wl2-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        results_b >= 1,
        "Expected at least 1 trade_result for window B, got {results_b}"
    );

    let open_a: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-wl2-a' AND status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        open_a, 0,
        "Expected 0 open trades for window A, got {open_a}"
    );

    let open_total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        open_total, 0,
        "Expected 0 open trades after both windows closed, got {open_total}"
    );
}

/// WL3: A window closes with no trades (no `CLOB` data sent) — `resolve_window`
/// is called with 0 trades and the bot returns Ok without errors.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_handles_close_with_no_trades_gracefully() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;

    tokio::time::sleep(Duration::from_secs(10)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    let run_result = inner.unwrap();
    assert!(
        run_result.is_ok(),
        "run_live returned an error when window closed with no trades: {run_result:?}"
    );

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let market_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert!(
        market_count > 0,
        "Expected at least 1 market in DB, got {market_count}"
    );

    let trade_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM simulated_trades", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        trade_count, 0,
        "Expected 0 trades (no CLOB data sent), got {trade_count}"
    );

    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        result_count, 0,
        "Expected 0 trade_results, got {result_count}"
    );

    let final_balance: f64 = conn
        .query_row(
            "SELECT balance FROM balance_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        (final_balance - 200.0).abs() < f64::EPSILON,
        "Expected balance to remain 200.0 (no trades), got {final_balance}"
    );

    let market_status: String = conn
        .query_row("SELECT status FROM markets LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(market_status, "closed");
}

/// WL4: Gamma can resolve later than the old one-shot retry window and trades
/// must still settle within the same live run.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_retries_authoritative_settlement_until_gamma_resolves() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);
    let unresolved = gamma_market_response(
        format!("btc-updown-5m-{current_slot}"),
        "mkt-sys-test",
        "cond-sys",
        "tok-up-sys",
        "tok-down-sys",
        end_date.as_str(),
        None,
    );
    let resolved = gamma_market_response(
        format!("btc-updown-5m-{current_slot}"),
        "mkt-sys-test",
        "cond-sys",
        "tok-up-sys",
        "tok-down-sys",
        end_date.as_str(),
        Some("UP"),
    );

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{current_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(unresolved.clone()))
        .up_to_n_times(2)
        .mount(&gamma_mock)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{current_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(resolved))
        .mount(&gamma_mock)
        .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.gamma_poll_interval = 60_000;
    config.min_window_time_ms = 0;
    config.latency_arb_momentum_threshold = 0.001;
    config.resolution_poll_retries = 0;
    config.resolution_initial_delay_ms = 0;
    config.resolution_poll_delay_ms = 1_000;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    for i in 25_u32..35 {
        let price = 42_000.0 + f64::from(i) * 5.0;
        let msg = format!(
            r#"{{"e":"aggTrade","E":{},"p":"{}","q":"0.01","T":{}}}"#,
            1_700_000_000_000_u64 + u64::from(i) * 100,
            price,
            1_700_000_000_000_u64 + u64::from(i) * 100,
        );
        binance_mock.send(&msg).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_secs(17)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let closed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-sys-test' AND status = 'closed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        closed >= 1,
        "expected a closed trade for mkt-sys-test, got {closed}"
    );

    let results: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM trade_results r \
             JOIN simulated_trades t ON r.trade_id = t.id \
             WHERE t.market_id = 'mkt-sys-test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        results >= 1,
        "expected a trade_result for mkt-sys-test, got {results}"
    );
}

/// WL5: Restarting the bot must backfill already-ended unresolved open trades
/// from the database without requiring a fresh run.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_backfills_unresolved_open_trade_on_startup() {
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ended_slot = ((now_secs / 300) * 300).saturating_sub(300);
    #[allow(clippy::cast_possible_wrap)]
    let ended_date = chrono::DateTime::from_timestamp(ended_slot as i64 + 300, 0)
        .unwrap()
        .to_rfc3339();

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{ended_slot}$"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(gamma_market_response(
                format!("btc-updown-5m-{ended_slot}"),
                "mkt-wl5",
                "cond-wl5",
                "tok-up-wl5",
                "tok-down-wl5",
                ended_date.as_str(),
                Some("UP"),
            )),
        )
        .mount(&gamma_mock)
        .await;

    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();
    {
        let db = buba_paint::db::database::Database::new(&db_path).unwrap();
        db.upsert_market(&buba_paint::types::MarketWindow {
            market_id: "mkt-wl5".to_string(),
            question: "Will BTC go up? (WL5)".to_string(),
            up_token_id: "tok-up-wl5".to_string(),
            down_token_id: "tok-down-wl5".to_string(),
            condition_id: "cond-wl5".to_string(),
            start_time: ended_slot.saturating_mul(1000),
            end_time: ended_slot.saturating_add(300).saturating_mul(1000),
            slug: format!("btc-updown-5m-{ended_slot}"),
            outcome: None,
            resolution_source: Some("chainlink".to_string()),
            fee_profile: Some("crypto".to_string()),
            order_min_size: Some(5.0),
            order_price_min_tick_size: Some(0.01),
            maker_base_fee: Some(1000.0),
            taker_base_fee: Some(1000.0),
            rewards_min_size: Some(50.0),
            rewards_max_spread: Some(4.5),
            fees_enabled: Some(true),
            fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".to_string()),
            token_fee_rates_json: Some("{\"tok-up-wl5\":{\"base_fee\":1000}}".to_string()),
            accepting_orders: Some(true),
            accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
            clear_book_on_start: Some(false),
        })
        .unwrap();
        db.open_trade(&buba_paint::types::SimulatedTrade {
            id: None,
            timestamp: ended_slot
                .saturating_add(300)
                .saturating_mul(1000)
                .saturating_sub(10_000),
            market_id: "mkt-wl5".to_string(),
            strategy: "latency-arb".to_string(),
            side: buba_paint::types::SignalDirection::Down,
            token_id: "tok-down-wl5".to_string(),
            entry_price: 0.52,
            size: 10.0,
            status: buba_paint::types::TradeStatus::Open,
            signal_id: None,
            requested_price: None,
            requested_size: None,
            filled_size: None,
            avg_fill_price: None,
            fill_status: None,
            fill_reason: None,
            fill_latency_ms: None,
            execution_group_id: None,
            execution_fidelity: None,
            execution_mode: None,
            order_id: None,
            fill_price: None,
        })
        .unwrap();
        db.close();
    }

    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.resolution_initial_delay_ms = 0;
    config.resolution_poll_delay_ms = 1_000;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_secs(2)).await;

    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM simulated_trades WHERE market_id = 'mkt-wl5' AND status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open_count, 0);

    let result_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM trade_results r \
             JOIN simulated_trades t ON r.trade_id = t.id \
             WHERE t.market_id = 'mkt-wl5'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(result_count, 1);
}
