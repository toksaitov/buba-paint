mod support;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        ..Config::default()
    }
}

/// Helper: register a Gamma API mock that returns a market ending at `end_date`
/// with the given `current_slot` slug.
async fn register_gamma_mock(gamma_mock: &MockServer, current_slot: u64, end_date: &str) {
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

/// Helper: send CLOB book snapshots with a low UP ask (triggers latency-arb).
async fn send_clob_book(clob_mock: &MockWsServer) {
    clob_mock
        .send(
            r#"{"asset_id":"tok-up-sys","timestamp":1700000000000,"bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.45","size":"100"}]}"#,
        )
        .await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-down-sys","timestamp":1700000000000,"bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.50","size":"100"}]}"#,
        )
        .await;
}

/// Helper: send a Chainlink reference price.
async fn send_chainlink_price(chainlink_mock: &MockWsServer) {
    chainlink_mock
        .send(
            r#"{"topic":"crypto_prices_chainlink","payload":{"value":42000,"timestamp":1700000000000}}"#,
        )
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

    // Wait for feeds to connect and discovery to find the market.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Send scripted messages.
    send_rising_binance_ticks(binance_mock, 25).await;
    send_chainlink_price(chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(clob_mock).await;

    // Send additional ticks after book arrives to trigger strategy evaluation.
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

    // Wait for the window to close (endDate was `window_secs` from start, plus margin).
    tokio::time::sleep(Duration::from_secs(window_secs + 2)).await;

    // Shutdown.
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: make the market window close ~4 seconds from now.
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let end_time_secs = now_secs + 4;
    #[allow(clippy::cast_possible_wrap)]
    let end_date = chrono::DateTime::from_timestamp(end_time_secs as i64, 0)
        .unwrap()
        .to_rfc3339();

    // Use the current 5-minute slot for the slug so discovery finds it.
    let current_slot = (now_secs / 300) * 300;

    // 3. Register Gamma API mock.
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

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config with mock URLs and fast timers.
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
        ..Config::default()
    };

    // 6. Start the live bot with shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect and discovery to find the market.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // 8. Send scripted messages.

    // Binance: rising prices to generate momentum.
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

    // Chainlink: send a reference price.
    chainlink_mock
        .send(
            r#"{"topic":"crypto_prices_chainlink","payload":{"value":42000,"timestamp":1700000000000}}"#,
        )
        .await;

    // CLOB: wait for the resubscription to have taken effect, then send book snapshots.
    tokio::time::sleep(Duration::from_millis(500)).await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-up-sys","timestamp":1700000000000,"bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.45","size":"100"}]}"#,
        )
        .await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-down-sys","timestamp":1700000000000,"bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.50","size":"100"}]}"#,
        )
        .await;

    // 9. Wait for the window to close (endDate was ~4s from start, plus margin).
    tokio::time::sleep(Duration::from_secs(5)).await;

    // 10. Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    // Unwrap the JoinHandle result and inner Result.
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    // 11. Verify database.
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Market should be upserted.
    let market_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert!(
        market_count > 0,
        "Expected at least 1 market, got {market_count}"
    );

    // Balance log should have at least an init entry.
    let balance_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM balance_log", [], |r| r.get(0))
        .unwrap();
    assert!(
        balance_count > 0,
        "Expected balance_log entries, got {balance_count}"
    );

    // Tick data should have been logged (tick logger runs every 200ms, we
    // waited ~7s total, and we sent binance + chainlink + clob data).
    let tick_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))
        .unwrap();
    assert!(
        tick_count > 0,
        "Expected tick_data entries, got {tick_count}"
    );
}

/// A1: Verify that the live bot actually executes trades, settles them on window
/// close, and records results in the database.  The existing test only checks
/// for market/balance\_log/tick\_data existence; this one verifies the full
/// trade lifecycle: open -> close -> `trade_results`.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_settles_trades_and_records_results() {
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: use an 8s window so there is plenty of time for the
    //    strategy to evaluate after all feeds deliver data.
    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    // 3. Register Gamma API.
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config with min_window_time_ms=0 to guarantee trade execution
    //    regardless of when the book data arrives relative to window end.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect and discovery to find the market.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // 8. Send scripted messages: rising Binance ticks for momentum.
    send_rising_binance_ticks(&binance_mock, 25).await;

    // Chainlink reference price.
    send_chainlink_price(&chainlink_mock).await;

    // Wait for CLOB resubscription, then send book snapshots.
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // Send additional ticks to trigger re-evaluation after the book arrives.
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

    // 9. Wait for the window to close (endDate ~8s from start, plus margin).
    tokio::time::sleep(Duration::from_secs(8)).await;

    // 10. Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    // 11. Verify database: trades were executed and settled.
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // At least one trade should have been opened and closed.
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

    // trade_results table should have matching entries.
    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert!(
        result_count >= 1,
        "Expected at least 1 trade result, got {result_count}"
    );

    // The final balance should differ from 200.0 (trade PnL applied).
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

    // Verify that trade_results have reasonable PnL values.
    let pnl: f64 = conn
        .query_row("SELECT pnl_0pct FROM trade_results LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    // PnL should be non-zero (either a win or a loss).
    assert!(pnl.abs() > f64::EPSILON, "Expected non-zero PnL, got {pnl}");

    // No open trades should remain after window close.
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: 8s window to give plenty of time for trade execution.
    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    // 3. Register Gamma API.
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config with a very short chainlink_stale_ms so the stale event
    //    fires quickly after we stop sending Chainlink updates.
    //    Also set min_window_time_ms=0 to guarantee trade execution.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.chainlink_stale_ms = 500;
    config.min_window_time_ms = 0;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // 8. Send ONE Chainlink price, then STOP.
    send_chainlink_price(&chainlink_mock).await;

    // Wait a bit, then send Binance ticks (these keep flowing the whole time).
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_rising_binance_ticks(&binance_mock, 25).await;

    // By now, >500ms since last Chainlink, so ChainlinkStale should have fired.
    // The chainlink feed will also reconnect, but that's fine.

    // Wait for CLOB resubscription, then send book.
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // More Binance ticks to trigger strategy evaluation after book arrives.
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

    // 9. Wait for window to close (8s window + margin).
    tokio::time::sleep(Duration::from_secs(8)).await;

    // 10. Shutdown.
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

    // 11. Verify database: trades were settled despite Chainlink going stale.
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // At least one trade should have been settled.
    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert!(
        result_count >= 1,
        "Expected at least 1 trade result even with stale Chainlink, got {result_count}"
    );

    // Settlement should have used a non-zero settlement price (0 or 1).
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

    // Verify no open trades remain.
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
    // === FIRST RUN: trade and establish a final balance ===

    // 1. Start mock servers (first run).
    let binance_mock_1 = MockWsServer::start().await;
    let clob_mock_1 = MockWsServer::start().await;
    let chainlink_mock_1 = MockWsServer::start().await;
    let gamma_mock_1 = MockServer::start().await;

    // Use an 8s window for the first run to allow trades to execute.
    let (_now_secs_1, current_slot_1, end_date_1) = compute_window_timing_with_offset(8);
    register_gamma_mock(&gamma_mock_1, current_slot_1, &end_date_1).await;

    // Use a temp file that we keep alive across both runs.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    let mut config_1 = test_config(
        &binance_mock_1.url,
        &clob_mock_1.url,
        &chainlink_mock_1.url,
        &gamma_mock_1.uri(),
    );
    config_1.min_window_time_ms = 0;

    // Run the first bot instance (8s window + 2s margin = 10s total wait).
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

    // Read the final balance from the DB after the first run.
    let first_run_final_balance: f64 = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT balance FROM balance_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };

    // The first run should have traded (balance differs from 200.0).
    // However, it's possible the trade happened but PnL was applied.
    // At minimum, the init balance log entry exists.
    let first_run_trade_count: i64 = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
            .unwrap()
    };

    // === SECOND RUN: use a DIFFERENT starting balance (999.0) ===

    // New mock servers for the second run (fresh connections).
    let binance_mock_2 = MockWsServer::start().await;
    let clob_mock_2 = MockWsServer::start().await;
    let chainlink_mock_2 = MockWsServer::start().await;
    let gamma_mock_2 = MockServer::start().await;

    let (_now_secs_2, current_slot_2, end_date_2) = compute_window_timing_with_offset(6);
    register_gamma_mock(&gamma_mock_2, current_slot_2, &end_date_2).await;

    let config_2 = test_config(
        &binance_mock_2.url,
        &clob_mock_2.url,
        &chainlink_mock_2.url,
        &gamma_mock_2.uri(),
    );

    // Start second bot instance with a DIFFERENT starting balance.
    #[allow(clippy::similar_names)]
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    let db_path2 = db_path.clone();
    let bot_handle_2 = tokio::spawn(async move {
        buba_paint::live::run_live(config_2, &db_path2, 999.0, stop_receiver).await
    });

    // Let it start up and recover balance from DB.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Send enough data for the second run to proceed through its window.
    send_rising_binance_ticks(&binance_mock_2, 20).await;
    send_chainlink_price(&chainlink_mock_2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock_2).await;

    // Wait for the window to close (6s window + margin).
    tokio::time::sleep(Duration::from_secs(8)).await;

    // Shutdown the second bot.
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

    // === VERIFY: the second run recovered from the DB, NOT from 999.0 ===
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Get all balance log entries to trace the recovery.
    let mut stmt = conn
        .prepare("SELECT event, balance FROM balance_log ORDER BY id ASC")
        .unwrap();
    let entries: Vec<(String, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    // There should be at least 2 entries (first run init + first run trade PnL).
    assert!(
        entries.len() >= 2,
        "Expected at least 2 balance_log entries, got {}",
        entries.len()
    );

    // The first "init" entry should be 200.0 (from the first run).
    assert!(
        (entries[0].1 - 200.0).abs() < f64::EPSILON,
        "First init balance should be 200.0, got {}",
        entries[0].1
    );

    // CRITICAL: No balance_log entry should be 999.0.
    // If the bot properly recovered from the DB, it should have used
    // the first run's final balance, not the 999.0 we passed.
    let has_999 = entries
        .iter()
        .any(|(_, bal)| (*bal - 999.0).abs() < f64::EPSILON);
    assert!(
        !has_999,
        "Balance 999.0 should NOT appear in balance_log — \
         the bot should have recovered from DB. Entries: {entries:?}"
    );

    // If the first run had trades, the recovered balance should match
    // the first run's final balance.
    if first_run_trade_count > 0 {
        // The second run's activity should be based on first_run_final_balance.
        // None of the balances should be exactly 999.0.
        let second_run_balances: Vec<f64> = entries
            .iter()
            .skip(1) // skip first init
            .map(|(_, bal)| *bal)
            .collect();
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing for window 1: closes in 6s from now.
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

    // Window 2 closes 12s from now (6s after window 1 closes).
    let end_time_2 = now_secs + 12;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_2 = chrono::DateTime::from_timestamp(end_time_2 as i64, 0)
        .unwrap()
        .to_rfc3339();

    // 3. Register Gamma mocks for both slots.
    // The discovery checks both current_slot and next_slot on each poll.
    // We register window 1 on current_slot and window 2 on next_slot
    // with different market IDs.
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

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect and discovery to find window 1.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // 8. Send ticks for window 1.
    send_rising_binance_ticks(&binance_mock, 20).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // 9. Wait for window 1 to close (6s from start + margin).
    tokio::time::sleep(Duration::from_secs(6)).await;

    // 10. Send ticks for window 2 (discovery should have already found it
    //     since it polls both current_slot and next_slot).
    send_rising_binance_ticks(&binance_mock, 20).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // 11. Wait for window 2 to close (another ~6s + margin).
    tokio::time::sleep(Duration::from_secs(8)).await;

    // 12. Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    // 13. Verify database: at least 2 distinct market_ids in `markets` table.
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

    // Verify both specific market IDs exist.
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

/// A5: Verify that the `StrategyResult::Batch` path (spread-capture strategy)
/// works end-to-end in the live bot: both legs of the spread are opened.
/// Covers live.rs lines 447-468 (Batch branch in `evaluate_strategies`).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_spread_capture_executes() {
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: 8s window.
    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    // 3. Register Gamma API.
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config:
    //    - spread_capture_threshold=0.90 (low enough to trigger with up_ask+down_ask < 0.90)
    //    - latency_arb_momentum_threshold=99.0 (disable latency-arb so only spread fires)
    //    - min_window_time_ms=0 to guarantee execution
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.spread_capture_threshold = 0.90;
    config.spread_capture_min_ask = 0.15;
    config.latency_arb_momentum_threshold = 99.0;
    config.min_window_time_ms = 0;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect and discovery.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // 8. Send Binance ticks (needed for binance_price to be set, but
    //    momentum is irrelevant since latency-arb threshold is 99.0).
    send_rising_binance_ticks(&binance_mock, 10).await;

    // Chainlink reference price.
    send_chainlink_price(&chainlink_mock).await;

    // 9. Wait for CLOB resubscription, then send a spread-friendly book:
    //    up_ask=0.40, down_ask=0.40 -> total=0.80 < 0.90 threshold.
    tokio::time::sleep(Duration::from_millis(500)).await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-up-sys","timestamp":1700000000000,"bids":[{"price":"0.35","size":"100"}],"asks":[{"price":"0.40","size":"100"}]}"#,
        )
        .await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-down-sys","timestamp":1700000000000,"bids":[{"price":"0.35","size":"100"}],"asks":[{"price":"0.40","size":"100"}]}"#,
        )
        .await;

    // 10. Send additional Binance ticks to trigger strategy evaluation after book.
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

    // 11. Wait for window close (8s + margin).
    tokio::time::sleep(Duration::from_secs(8)).await;

    // 12. Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    // 13. Verify database: spread-capture should have opened both legs.
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

    // Verify both directions were opened.
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

    // Verify that no latency-arb trades were opened (we disabled it).
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing for two windows within a single bot run.
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

    // 3. Register ONLY window 1 initially (uses wildcard regex; both slots
    //    will match but return the same market_id, which is fine).
    register_gamma_mock(&gamma_mock, current_slot, &end_date_1).await;

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config: CB triggers after 1 loss with a 120s pause.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.circuit_breaker_losses = 1;
    config.circuit_breaker_pause_ms = 120_000;
    config.min_window_time_ms = 0;

    // 6. Start the bot (single instance for both windows).
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect and discovery.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // === WINDOW 1: trigger a losing UP trade ===
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // More ticks to trigger eval after book arrives.
    tokio::time::sleep(Duration::from_millis(300)).await;
    send_rising_binance_ticks(&binance_mock, 10).await;

    // Send FALLING ticks so close_price < open_price at window 1 end.
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

    // 8. Wait for window 1 to close. Trade loses -> CB fires.
    tokio::time::sleep(Duration::from_secs(12)).await;

    // === WINDOW 2: mount a fresh Gamma mock so discovery finds a new market ===
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

    // Wait for discovery to pick up window 2 + CLOB resubscription.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Send ticks that WOULD trigger trades if CB were not blocking.
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    send_rising_binance_ticks(&binance_mock, 15).await;

    // 9. Wait for window 2 to close.
    tokio::time::sleep(Duration::from_secs(14)).await;

    // 10. Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned error");

    // 11. Verify DB state.
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Window 1 (mkt-sys-test) should have at least 1 trade.
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

    // Window 2 (mkt-cb-w2) should have 0 trades (CB blocked them).
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

    // Verify trade side is UP.
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

    // Verify settlement_price=1.0 (UP wins when close >= open).
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

    // Verify pnl_0pct > 0.
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

    // Use unique market/token IDs for this test.
    Mock::given(method("GET"))
        .and(path_regex(r"^/events/slug/btc-updown-5m-\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{current_slot}"),
            "markets": [{
                "id": "mkt-ts2-down",
                "question": "Will BTC go up?",
                "conditionId": "cond-ts2",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-ts2", "tok-down-ts2"],
                "endDate": end_date
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
    // Use a low momentum threshold to make it easier to trigger.
    config.latency_arb_momentum_threshold = 0.0005;
    // Allow DOWN asks in the acceptable range.
    config.latency_arb_max_ask = 0.65;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // Wait for feeds to connect and discovery.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Phase 1: Send high initial Binance prices. These will be captured as
    // open_price via window_open_prices.entry().or_insert().
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

    // Send Chainlink price and CLOB book.
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // CLOB book with DOWN ask in acceptable range.
    clob_mock
        .send(
            r#"{"asset_id":"tok-up-ts2","timestamp":1700000000000,"bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.50","size":"100"}]}"#,
        )
        .await;
    clob_mock
        .send(
            r#"{"asset_id":"tok-down-ts2","timestamp":1700000000000,"bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.45","size":"100"}]}"#,
        )
        .await;

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Phase 2: Send falling Binance prices to create negative momentum.
    // These prices are well below the open_price (~42000), so at settlement
    // close < open means DOWN wins.
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

    // Wait for window to close.
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

    // DOWN wins when close < open, so settlement_price = 1.0.
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

    // Read entry_price and size from the trade.
    let (entry_price, size): (f64, f64) = conn
        .query_row(
            "SELECT entry_price, size FROM simulated_trades WHERE status = 'closed' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    // Read actual pnl_0pct and settlement_price from results.
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

/// SC1: Verify that book state resets between windows. Window A has CLOB data
/// and trades; window B has NO CLOB data sent for its tokens, so the stale
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

    // Register window A on current_slot only (not wildcard).
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

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Window A: send ticks + CLOB book (uses tok-up-sys/tok-down-sys).
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // Send additional ticks with unique timestamps to trigger strategy evaluation.
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

    // Wait for window A to close.
    tokio::time::sleep(Duration::from_secs(12)).await;

    // Mount window B with different token IDs.
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

    // Send ticks for B but NO CLOB data for B's tokens.
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

    // Window A (mkt-sys-test) should have trades.
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

    // Window B (mkt-sc1-b) should have 0 trades (no CLOB data, book was reset).
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

    // Wait for feeds to connect and discovery.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Send Chainlink + CLOB but NO Binance ticks at all.
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // Wait for the window to close.
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

    let end_time_b = now_secs + 22;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    // Register window A on current_slot only.
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

    // Window A: rising ticks to open UP trade.
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

    // Falling ticks so close < open (UP loses).
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

    // Wait for window A to close.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Mount window B.
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

    // Window B: send all data.
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
/// Window A: trade loses -> CB triggers (`circuit_breaker_losses`=1, pause=6s).
/// Window B: starts after CB pause expires -> new trade opens.
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

    let end_time_b = now_secs + 22;
    #[allow(clippy::cast_possible_wrap)]
    let end_date_b = chrono::DateTime::from_timestamp(end_time_b as i64, 0)
        .unwrap()
        .to_rfc3339();

    // Register window A on current_slot only.
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
    config.peak_dd_pause_pct = 1.0;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Window A: open UP trade, then crash price.
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book(&clob_mock).await;

    // Send additional ticks with unique timestamps to trigger strategy evaluation.
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

    // Falling ticks so close < open (UP loses).
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

    // Wait for window A close + CB to expire (8s close + 8s CB expiry).
    tokio::time::sleep(Duration::from_secs(8)).await;
    tokio::time::sleep(Duration::from_secs(8)).await;

    // Mount window B after CB expired.
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

    // Window B: send data after CB expired.
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Register Gamma with a window far in the future so the bot stays alive.
    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(30);
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    // 3. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 4. Build config with fast reconnect to speed up the test.
    let config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );

    // 5. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 6. Wait for feeds to connect.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 7. Close all 3 mock WS servers simultaneously.
    binance_mock.close().await;
    clob_mock.close().await;
    chainlink_mock.close().await;

    // 8. Wait for the bot to process the disconnections and attempt reconnects.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 9. Verify bot is still running (hasn't panicked) by sending shutdown.
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

// ---------------------------------------------------------------------------
// WL: Window Lifecycle integration tests
// ---------------------------------------------------------------------------

/// Helper: send a CLOB book snapshot for custom token IDs.
///
/// Sends both UP and DOWN snapshots as a JSON array so the CLOB feed builds
/// a full `BookState` (with both sides) in a single WebSocket frame.  This
/// avoids the 200ms eval throttle eating the first `ClobBook` event (up-only)
/// and leaving the second (up+down) throttled.
async fn send_clob_book_for_tokens(clob_mock: &MockWsServer, up_token: &str, down_token: &str) {
    clob_mock
        .send(&format!(
            r#"[{{"asset_id":"{up_token}","timestamp":1700000000000,"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.45","size":"100"}}]}},{{"asset_id":"{down_token}","timestamp":1700000000000,"bids":[{{"price":"0.40","size":"100"}}],"asks":[{{"price":"0.50","size":"100"}}]}}]"#,
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: window A ends T+15s, window B ends T+30s.
    //    The B discovery is delayed 8s so there is plenty of time (~5s) for
    //    trades to open in A before B's discovery resets book state.
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

    // 3. Register Gamma mocks with path-specific matchers.
    //    Window A (current_slot) responds immediately.
    //    Window B (next_slot) is delayed 8 seconds — simulating the production
    //    race where discovery finds B before A's `WindowClosed` fires.
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{current_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{current_slot}"),
            "markets": [{
                "id": "mkt-wl1-a",
                "question": "Will BTC go up? (WL1 window A)",
                "conditionId": "cond-wl1-a",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-wl1a", "tok-down-wl1a"],
                "endDate": end_date_a
            }]
        })))
        .mount(&gamma_mock)
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

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.latency_arb_momentum_threshold = 0.001;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // --- Timeline ---
    // T=0: Bot starts, discovery finds window A immediately.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // T~2s: Send rising Binance ticks + Chainlink + CLOB book for A's tokens.
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book_for_tokens(&clob_mock, "tok-up-wl1a", "tok-down-wl1a").await;

    // Send additional ticks to trigger strategy evaluation after book arrives.
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

    // T~8.5s: Discovery finds window B (8s delay on the Gamma response).
    //         current_window is now B, but A still has open trades.
    //         known_windows = {A, B}.

    // T=15s: WindowClosed(A) fires.  The fix uses known_windows.remove("A")
    //        to find the window and settle trades, even though current_window=B.

    // Wait for window A to close (15s from start + 2s margin).
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    // --- Verify database ---
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Window A's trades must be settled (status='closed').
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

    // trade_results must exist for window A.
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

    // Balance should differ from starting 200.0 (trade PnL applied).
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

    // No open trades should remain for window A.
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
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: A ends T+8s, B ends T+18s.
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

    // 3. Gamma mocks: both slots respond immediately.
    //    A uses current_slot, B uses next_slot.
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

    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/events/slug/btc-updown-5m-{next_slot}$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": format!("btc-updown-5m-{next_slot}"),
            "markets": [{
                "id": "mkt-wl2-b",
                "question": "Will BTC go up? (WL2 window B)",
                "conditionId": "cond-wl2-b",
                "outcomes": ["Up", "Down"],
                "clobTokenIds": ["tok-up-wl2b", "tok-down-wl2b"],
                "endDate": end_date_b
            }]
        })))
        .mount(&gamma_mock)
        .await;

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;
    config.latency_arb_momentum_threshold = 0.001;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // --- Timeline ---
    // T=0: Bot starts. Discovery finds both A and B immediately (no delay).
    //      CLOB subscribes to B's tokens (B is the last discovered, becomes current).
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // T~2s: Send data for window B (the current window after both are discovered).
    //       Discovery processes both slots: A first (NewWindow A), then B (NewWindow B).
    //       current_window is now B. CLOB is subscribed to B's tokens.
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_clob_book_for_tokens(&clob_mock, "tok-up-wl2b", "tok-down-wl2b").await;

    // Additional ticks to trigger evaluation.
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

    // T=8s: WindowClosed(A) fires (A had no trades, just verifies graceful close).
    // T=18s: WindowClosed(B) fires -> B settled.

    // Wait for both windows to close (18s from start + 4s margin).
    tokio::time::sleep(Duration::from_secs(16)).await;

    // Shutdown.
    let _ = shutdown_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), bot_handle).await;
    assert!(
        result.is_ok(),
        "bot did not shut down within timeout after shutdown signal"
    );
    let inner = result.unwrap();
    assert!(inner.is_ok(), "bot task panicked: {inner:?}");
    assert!(inner.unwrap().is_ok(), "run_live returned an error");

    // --- Verify database ---
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Both markets should be upserted.
    let market_count: i64 = conn
        .query_row("SELECT COUNT(DISTINCT market_id) FROM markets", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        market_count >= 2,
        "Expected at least 2 markets, got {market_count}"
    );

    // Window B (the current window, where trades open): settled.
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

    // Window B: trade_results exist.
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

    // Window A: closes gracefully with 0 trades (no CLOB data for A's tokens).
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

    // No open trades should remain for either window.
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

/// WL3: A window closes with no trades (no CLOB data sent) — `resolve_window`
/// is called with 0 trades and the bot returns Ok without errors.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_bot_handles_close_with_no_trades_gracefully() {
    // 1. Start mock servers.
    let binance_mock = MockWsServer::start().await;
    let clob_mock = MockWsServer::start().await;
    let chainlink_mock = MockWsServer::start().await;
    let gamma_mock = MockServer::start().await;

    // 2. Compute timing: window ends T+8s.
    let (_now_secs, current_slot, end_date) = compute_window_timing_with_offset(8);

    // 3. Register Gamma API.
    register_gamma_mock(&gamma_mock, current_slot, &end_date).await;

    // 4. Create temp DB.
    let tmp_db = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp_db.path().to_str().unwrap().to_string();

    // 5. Build config with min_window_time_ms=0.
    let mut config = test_config(
        &binance_mock.url,
        &clob_mock.url,
        &chainlink_mock.url,
        &gamma_mock.uri(),
    );
    config.min_window_time_ms = 0;

    // 6. Start the live bot.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let db_path_clone = db_path.clone();
    let bot_handle = tokio::spawn(async move {
        buba_paint::live::run_live(config, &db_path_clone, 200.0, shutdown_rx).await
    });

    // 7. Wait for feeds to connect and discovery to find the market.
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // 8. Send Binance ticks and Chainlink -- but NO CLOB book.
    //    Without book data the strategies cannot fire (no ask prices).
    send_rising_binance_ticks(&binance_mock, 25).await;
    send_chainlink_price(&chainlink_mock).await;

    // DO NOT send any CLOB data. The strategy cannot produce signals
    // without up/down ask prices.

    // 9. Wait for the window to close (8s + margin).
    tokio::time::sleep(Duration::from_secs(10)).await;

    // 10. Shutdown.
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

    // 11. Verify database.
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Market should exist in the DB (discovery found it and upserted it).
    let market_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert!(
        market_count > 0,
        "Expected at least 1 market in DB, got {market_count}"
    );

    // 0 trades should exist (no CLOB data => no signals => no trades).
    let trade_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM simulated_trades", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        trade_count, 0,
        "Expected 0 trades (no CLOB data sent), got {trade_count}"
    );

    // 0 trade results.
    let result_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trade_results", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        result_count, 0,
        "Expected 0 trade_results, got {result_count}"
    );

    // Balance should still be 200.0 (no trades = no PnL changes).
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
}
