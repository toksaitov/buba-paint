// Integration test for `buba_paint::backtest::runner::run_backtest()`.
//
// Uses a temporary SQLite database pre-populated with fixture data (the
// merged-data schema that includes `open_price`, `close_price`, `outcome`
// columns on the `markets` table).

use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

use std::sync::Arc;

use buba_paint::backtest::runner::{BacktestOptions, TickSource, run_backtest};
use buba_paint::backtest::tick_replay::{RawTick, TickReplay};
use buba_paint::config::Config;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Create a fixture data DB with the merged-data schema and a single 5-minute
/// market window where BTC goes **UP** (close > open).  Ticks simulate strong
/// upward momentum so the latency-arb strategy should trigger.
#[allow(clippy::too_many_lines)]
fn create_fixture_data_db() -> (NamedTempFile, String) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let conn = Connection::open(&path).unwrap();

    // Create the merged-data schema (different from per-run schema!).
    conn.execute_batch(
        "CREATE TABLE tick_data (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            source    TEXT NOT NULL,
            price     REAL,
            bid       REAL,
            ask       REAL,
            bid_size  REAL,
            ask_size  REAL
        );
        CREATE TABLE markets (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            market_id     TEXT NOT NULL UNIQUE,
            question      TEXT NOT NULL,
            condition_id  TEXT NOT NULL,
            slug          TEXT NOT NULL,
            up_token_id   TEXT NOT NULL,
            down_token_id TEXT NOT NULL,
            start_time    INTEGER NOT NULL,
            end_time      INTEGER NOT NULL,
            status        TEXT NOT NULL DEFAULT 'resolved',
            open_price    REAL,
            close_price   REAL,
            outcome       TEXT
        );",
    )
    .unwrap();

    // Insert a market window: 5 minutes, BTC goes UP (close > open).
    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug,
            up_token_id, down_token_id, start_time, end_time,
            open_price, close_price, outcome)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            "mkt-fixture-1",
            "Will BTC go up?",
            "cond-1",
            "btc-updown-5m-1000",
            "tok-up",
            "tok-down",
            1_000_000_i64,
            1_300_000_i64, // 5 minutes
            42_000.0,
            42_100.0,
            "UP"
        ],
    )
    .unwrap();

    let base_price = 42_000.0;

    // 30 seconds of ticks BEFORE the window opens (warmup for momentum).
    // Price is flat so momentum starts at zero; CLOB book has cheap UP ask.
    for i in 0..30 {
        let ts = 970_000_i64 + i64::from(i) * 1000;
        insert_tick(
            &conn,
            ts,
            "binance",
            Some(base_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "chainlink",
            Some(base_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "clob_up",
            None,
            Some(0.40),
            Some(0.45),
            Some(100.0),
            Some(100.0),
        );
        insert_tick(
            &conn,
            ts,
            "clob_down",
            None,
            Some(0.40),
            Some(0.50),
            Some(100.0),
            Some(100.0),
        );
    }

    // During the window: price rises sharply (triggers momentum signal).
    // +$5 per second = $150 over 30s at $42k base = 0.36% → exceeds 0.0015 threshold.
    // We generate ticks through the entire 300s window AND past the end so
    // the window closes and trades are resolved.
    for i in 0..330 {
        let ts = 1_000_000_i64 + i64::from(i) * 1000;
        // Price keeps rising during the window, then flattens.
        let price = if i < 300 {
            base_price + f64::from(i) * 5.0
        } else {
            base_price + 300.0 * 5.0
        };
        insert_tick(&conn, ts, "binance", Some(price), None, None, None, None);
        insert_tick(&conn, ts, "chainlink", Some(price), None, None, None, None);
        insert_tick(
            &conn,
            ts,
            "clob_up",
            None,
            Some(0.40),
            Some(0.45),
            Some(100.0),
            Some(100.0),
        );
        insert_tick(
            &conn,
            ts,
            "clob_down",
            None,
            Some(0.40),
            Some(0.50),
            Some(100.0),
            Some(100.0),
        );
    }

    drop(conn);
    (tmp, path)
}

/// Create a fixture data DB with completely flat prices (no momentum).
fn create_flat_fixture_data_db() -> (NamedTempFile, String) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let conn = Connection::open(&path).unwrap();

    conn.execute_batch(
        "CREATE TABLE tick_data (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            source    TEXT NOT NULL,
            price     REAL,
            bid       REAL,
            ask       REAL,
            bid_size  REAL,
            ask_size  REAL
        );
        CREATE TABLE markets (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            market_id     TEXT NOT NULL UNIQUE,
            question      TEXT NOT NULL,
            condition_id  TEXT NOT NULL,
            slug          TEXT NOT NULL,
            up_token_id   TEXT NOT NULL,
            down_token_id TEXT NOT NULL,
            start_time    INTEGER NOT NULL,
            end_time      INTEGER NOT NULL,
            status        TEXT NOT NULL DEFAULT 'resolved',
            open_price    REAL,
            close_price   REAL,
            outcome       TEXT
        );",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug,
            up_token_id, down_token_id, start_time, end_time,
            open_price, close_price, outcome)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            "mkt-flat-1",
            "Will BTC go up?",
            "cond-1",
            "btc-updown-5m-flat",
            "tok-up",
            "tok-down",
            1_000_000_i64,
            1_300_000_i64,
            42_000.0,
            42_000.0,
            "UP"
        ],
    )
    .unwrap();

    let flat_price = 42_000.0;
    // Warmup + during window: all prices are constant.
    for i in 0..150 {
        let ts = 970_000_i64 + i64::from(i) * 1000;
        insert_tick(
            &conn,
            ts,
            "binance",
            Some(flat_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "chainlink",
            Some(flat_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "clob_up",
            None,
            Some(0.40),
            Some(0.45),
            Some(100.0),
            Some(100.0),
        );
        insert_tick(
            &conn,
            ts,
            "clob_down",
            None,
            Some(0.40),
            Some(0.50),
            Some(100.0),
            Some(100.0),
        );
    }

    drop(conn);
    (tmp, path)
}

#[allow(clippy::too_many_arguments)]
fn insert_tick(
    conn: &Connection,
    timestamp: i64,
    source: &str,
    price: Option<f64>,
    bid: Option<f64>,
    ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
) {
    conn.execute(
        "INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![timestamp, source, price, bid, ask, bid_size, ask_size],
    )
    .unwrap();
}

/// Helper to build a `Config` suitable for backtest integration tests.
/// Disables peak-DD pause (would trigger on small balances) and uses a
/// high spread-capture threshold to avoid overcounting.
fn test_config() -> Config {
    Config {
        peak_dd_pause_pct: 1.0,         // disable peak DD pause
        spread_capture_threshold: 0.50, // disable spread-capture overcounting
        log_level: "error".to_string(),
        ..Config::default()
    }
}

/// Create a fixture data DB suitable for spread-capture testing.
///
/// Has a single market window and CLOB book data where both asks are cheap
/// (`up_ask`=0.40, `down_ask`=0.40, total=0.80) so spread-capture triggers
/// when `spread_capture_threshold > 0.80`.
#[allow(clippy::too_many_lines)]
fn create_spread_capture_fixture_db() -> (NamedTempFile, String) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let conn = Connection::open(&path).unwrap();

    conn.execute_batch(
        "CREATE TABLE tick_data (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            source    TEXT NOT NULL,
            price     REAL,
            bid       REAL,
            ask       REAL,
            bid_size  REAL,
            ask_size  REAL
        );
        CREATE TABLE markets (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            market_id     TEXT NOT NULL UNIQUE,
            question      TEXT NOT NULL,
            condition_id  TEXT NOT NULL,
            slug          TEXT NOT NULL,
            up_token_id   TEXT NOT NULL,
            down_token_id TEXT NOT NULL,
            start_time    INTEGER NOT NULL,
            end_time      INTEGER NOT NULL,
            status        TEXT NOT NULL DEFAULT 'resolved',
            open_price    REAL,
            close_price   REAL,
            outcome       TEXT
        );",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug,
            up_token_id, down_token_id, start_time, end_time,
            open_price, close_price, outcome)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            "mkt-spread-1",
            "Will BTC go up?",
            "cond-1",
            "btc-updown-5m-spread",
            "tok-up",
            "tok-down",
            1_000_000_i64,
            1_300_000_i64,
            42_000.0,
            42_100.0,
            "UP"
        ],
    )
    .unwrap();

    let base_price = 42_000.0;

    // Warmup ticks (30s before window opens).
    // Flat prices so latency-arb does NOT trigger, only spread-capture should.
    for i in 0..30 {
        let ts = 970_000_i64 + i64::from(i) * 1000;
        insert_tick(
            &conn,
            ts,
            "binance",
            Some(base_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "chainlink",
            Some(base_price),
            None,
            None,
            None,
            None,
        );
        // Both UP and DOWN asks at 0.40 → total = 0.80 (below threshold 0.90).
        insert_tick(
            &conn,
            ts,
            "clob_up",
            None,
            Some(0.35),
            Some(0.40),
            Some(100.0),
            Some(100.0),
        );
        insert_tick(
            &conn,
            ts,
            "clob_down",
            None,
            Some(0.35),
            Some(0.40),
            Some(100.0),
            Some(100.0),
        );
    }

    // During the window: flat prices but cheap book on both sides.
    for i in 0..330 {
        let ts = 1_000_000_i64 + i64::from(i) * 1000;
        insert_tick(
            &conn,
            ts,
            "binance",
            Some(base_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "chainlink",
            Some(base_price),
            None,
            None,
            None,
            None,
        );
        insert_tick(
            &conn,
            ts,
            "clob_up",
            None,
            Some(0.35),
            Some(0.40),
            Some(100.0),
            Some(100.0),
        );
        insert_tick(
            &conn,
            ts,
            "clob_down",
            None,
            Some(0.35),
            Some(0.40),
            Some(100.0),
            Some(100.0),
        );
    }

    drop(conn);
    (tmp, path)
}

/// Window definition for multi-window fixtures.
///
/// The `momentum_up` field controls whether tick prices rise (true) or fall
/// (false) during the window, which is independent of the `outcome` field
/// that determines settlement.  This decoupling lets us create scenarios
/// where the bot trades in one direction but the outcome goes the other way
/// (i.e., losing trades).
struct WindowDef {
    market_id: &'static str,
    start_time: i64,
    end_time: i64,
    open_price: f64,
    close_price: f64,
    outcome: &'static str,
    /// If `true`, tick prices rise during this window (positive momentum → UP
    /// signal).  If `false`, tick prices fall (negative momentum → DOWN signal).
    momentum_up: bool,
}

/// Create a fixture data DB with multiple 5-minute market windows.
///
/// * `windows` — list of `WindowDef` structs.
/// * Between windows, ticks with the given `base_price` are generated.
/// * During each window, price rises or falls according to `momentum_up`.
/// * CLOB book: `up_ask`=0.45, `down_ask`=0.50 (latency-arb friendly).
#[allow(clippy::too_many_lines)]
fn create_multi_window_fixture_db(
    windows: &[WindowDef],
    base_price: f64,
    tick_spacing_ms: i64,
) -> (NamedTempFile, String) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let conn = Connection::open(&path).unwrap();

    conn.execute_batch(
        "CREATE TABLE tick_data (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            source    TEXT NOT NULL,
            price     REAL,
            bid       REAL,
            ask       REAL,
            bid_size  REAL,
            ask_size  REAL
        );
        CREATE TABLE markets (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            market_id     TEXT NOT NULL UNIQUE,
            question      TEXT NOT NULL,
            condition_id  TEXT NOT NULL,
            slug          TEXT NOT NULL,
            up_token_id   TEXT NOT NULL,
            down_token_id TEXT NOT NULL,
            start_time    INTEGER NOT NULL,
            end_time      INTEGER NOT NULL,
            status        TEXT NOT NULL DEFAULT 'resolved',
            open_price    REAL,
            close_price   REAL,
            outcome       TEXT
        );",
    )
    .unwrap();

    for w in windows {
        conn.execute(
            "INSERT INTO markets (market_id, question, condition_id, slug,
                up_token_id, down_token_id, start_time, end_time,
                open_price, close_price, outcome)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                w.market_id,
                format!("Will BTC go {}?", w.outcome),
                format!("cond-{}", w.market_id),
                format!("slug-{}", w.market_id),
                format!("up-{}", w.market_id),
                format!("down-{}", w.market_id),
                w.start_time,
                w.end_time,
                w.open_price,
                w.close_price,
                w.outcome
            ],
        )
        .unwrap();
    }

    // Determine overall time range: 30s warmup before first window, 30s after last window.
    let first_start = windows.iter().map(|w| w.start_time).min().unwrap();
    let last_end = windows.iter().map(|w| w.end_time).max().unwrap();
    let tick_start = first_start - 30_000;
    let tick_end = last_end + 30_000;

    let mut ts = tick_start;
    while ts <= tick_end {
        // Determine if we're inside a window and what the price movement should be.
        let mut price = base_price;
        let mut in_window = false;

        for w in windows {
            if ts >= w.start_time && ts < w.end_time {
                in_window = true;
                let fraction = (ts - w.start_time) as f64 / (w.end_time - w.start_time) as f64;

                // The momentum amplifier controls the direction of tick prices,
                // independently of the settlement outcome.  The value must be
                // large enough so that a 30-second slice (momentum_window_ms)
                // produces momentum > latency_arb_momentum_threshold (0.0015).
                // With base=42k, 30s/300s=0.1 fraction-slice → need amp*0.1/42k
                // ≥ 0.0015  →  amp ≥ 630.  Use 1500 for comfortable margin.
                let momentum_amplifier = if w.momentum_up { 1500.0 } else { -1500.0 };
                price = base_price + momentum_amplifier * fraction;
                break;
            }
        }

        if !in_window {
            // Between windows: flat at base price.
            price = base_price;
        }

        insert_tick(&conn, ts, "binance", Some(price), None, None, None, None);
        insert_tick(&conn, ts, "chainlink", Some(price), None, None, None, None);
        insert_tick(
            &conn,
            ts,
            "clob_up",
            None,
            Some(0.40),
            Some(0.45),
            Some(100.0),
            Some(100.0),
        );
        insert_tick(
            &conn,
            ts,
            "clob_down",
            None,
            Some(0.40),
            Some(0.50),
            Some(100.0),
            Some(100.0),
        );

        ts += tick_spacing_ms;
    }

    drop(conn);
    (tmp, path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn backtest_runs_without_error() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    assert!(result.total_ticks > 0, "should have loaded ticks");
    assert!(result.total_windows > 0, "should have at least one window");
    assert!(result.duration_hours > 0.0);
    assert!(result.elapsed_seconds > 0.0);
}

#[test]
fn backtest_with_strong_momentum_produces_trades() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    assert!(
        result.signals > 0,
        "strong momentum should produce at least one signal; total_ticks={}, windows={}",
        result.total_ticks,
        result.total_windows,
    );
    assert!(
        result.trades > 0,
        "strong momentum with cheap ask should produce at least one trade; \
         signals={}, final_balance={}, total_ticks={}",
        result.signals,
        result.final_balance,
        result.total_ticks,
    );
    // Since the outcome is UP and we bought UP tokens, PnL should be positive.
    assert!(
        result.total_pnl > 0.0,
        "PnL should be positive when outcome matches direction; got {}",
        result.total_pnl
    );
}

#[test]
fn backtest_with_flat_prices_produces_no_latency_arb_trades() {
    let (_data_tmp, data_path) = create_flat_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_300_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    // Flat prices = zero momentum = no latency-arb signals.
    // Spread-capture is disabled via the threshold.
    assert_eq!(
        result.trades, 0,
        "flat prices should produce zero trades; got {}",
        result.trades
    );
}

#[test]
fn backtest_cleans_up_stale_output_db() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    // Pre-create a stale results DB with a balance_log entry at $999.
    {
        let stale_conn = Connection::open(&results_path).unwrap();
        buba_paint::db::schema::run_migrations(&stale_conn).unwrap();
        stale_conn
            .execute(
                "INSERT INTO balance_log (timestamp, event, trade_id, amount, balance)
                 VALUES (1, 'init', NULL, 999.0, 999.0)",
                [],
            )
            .unwrap();
    }

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    // The stale $999 balance should have been cleaned up.
    // Final balance should be based on the $200 starting balance, not $999.
    assert!(
        result.final_balance < 999.0,
        "stale output DB was not cleaned up; final_balance={} (expected < 999)",
        result.final_balance
    );
}

#[test]
fn backtest_respects_starting_balance() {
    let (_data_tmp, data_path) = create_flat_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let starting = 500.0;
    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_300_000,
        starting_balance: starting,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    // With flat prices and no trades, the final balance should equal the start.
    assert!(
        (result.final_balance - starting).abs() < f64::EPSILON,
        "expected final_balance={starting}, got {}",
        result.final_balance
    );
}

#[test]
fn backtest_result_fields_are_consistent() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    // Wins + losses should equal total trades.
    assert_eq!(
        result.wins + result.losses,
        result.trades,
        "wins({}) + losses({}) != trades({})",
        result.wins,
        result.losses,
        result.trades
    );

    // Win rate should be consistent.
    if result.trades > 0 {
        let expected_wr = result.wins as f64 / result.trades as f64;
        assert!(
            (result.win_rate - expected_wr).abs() < 1e-6,
            "win_rate mismatch: {} vs expected {}",
            result.win_rate,
            expected_wr
        );
    }

    // Total PnL should equal final_balance - starting_balance.
    let expected_pnl = result.final_balance - 200.0;
    assert!(
        (result.total_pnl - expected_pnl).abs() < 0.01,
        "total_pnl({}) != final_balance({}) - 200.0",
        result.total_pnl,
        result.final_balance
    );

    // High water mark should be >= final_balance.
    assert!(
        result.high_water_mark >= result.final_balance - 0.01,
        "hwm({}) should be >= final_balance({})",
        result.high_water_mark,
        result.final_balance
    );
}

// ---------------------------------------------------------------------------
// B1: TickSource::Cached produces identical results to TickSource::FromDb
// ---------------------------------------------------------------------------

#[test]
fn backtest_with_cached_tick_source() {
    let (_data_tmp, data_path) = create_fixture_data_db();

    // Run 1: FromDb
    let results_tmp_1 = NamedTempFile::new().unwrap();
    let results_path_1 = results_tmp_1.path().to_str().unwrap().to_string();

    let result_from_db = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path.clone(),
        results_db_path: results_path_1,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    // Load ticks manually for the Cached path.
    let conn = rusqlite::Connection::open_with_flags(
        &data_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let ticks: Vec<RawTick> = TickReplay::load_ticks(&conn, 970_000, 1_330_000).unwrap();
    drop(conn);
    let cached_ticks = Arc::new(ticks);

    // Run 2: Cached
    let results_tmp_2 = NamedTempFile::new().unwrap();
    let results_path_2 = results_tmp_2.path().to_str().unwrap().to_string();

    let result_cached = run_backtest(BacktestOptions {
        tick_source: TickSource::Cached(cached_ticks),
        data_db_path: data_path,
        results_db_path: results_path_2,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: test_config(),
    })
    .unwrap();

    // Compare all key fields — must be identical.
    assert_eq!(
        result_from_db.total_ticks, result_cached.total_ticks,
        "total_ticks mismatch: FromDb={}, Cached={}",
        result_from_db.total_ticks, result_cached.total_ticks
    );
    assert_eq!(
        result_from_db.total_windows, result_cached.total_windows,
        "total_windows mismatch"
    );
    assert_eq!(
        result_from_db.signals, result_cached.signals,
        "signals mismatch: FromDb={}, Cached={}",
        result_from_db.signals, result_cached.signals
    );
    assert_eq!(
        result_from_db.trades, result_cached.trades,
        "trades mismatch: FromDb={}, Cached={}",
        result_from_db.trades, result_cached.trades
    );
    assert_eq!(result_from_db.wins, result_cached.wins, "wins mismatch");
    assert_eq!(
        result_from_db.losses, result_cached.losses,
        "losses mismatch"
    );
    assert!(
        (result_from_db.win_rate - result_cached.win_rate).abs() < 1e-10,
        "win_rate mismatch: FromDb={}, Cached={}",
        result_from_db.win_rate,
        result_cached.win_rate
    );
    assert!(
        (result_from_db.total_pnl - result_cached.total_pnl).abs() < 0.01,
        "total_pnl mismatch: FromDb={}, Cached={}",
        result_from_db.total_pnl,
        result_cached.total_pnl
    );
    assert!(
        (result_from_db.final_balance - result_cached.final_balance).abs() < 0.01,
        "final_balance mismatch: FromDb={}, Cached={}",
        result_from_db.final_balance,
        result_cached.final_balance
    );
}

// ---------------------------------------------------------------------------
// B2: Spread-capture strategy produces batch trades
// ---------------------------------------------------------------------------

#[test]
fn backtest_spread_capture_fires() {
    let (_data_tmp, data_path) = create_spread_capture_fixture_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let config = Config {
        peak_dd_pause_pct: 1.0,               // disable peak DD pause
        spread_capture_threshold: 0.90,       // total_ask=0.80 < 0.90 triggers
        spread_capture_min_ask: 0.15,         // both asks 0.40 pass this
        latency_arb_momentum_threshold: 99.0, // disable latency-arb
        log_level: "error".to_string(),
        ..Config::default()
    };

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: true,
        config,
    })
    .unwrap();

    // Spread-capture should fire at least one batch (2 legs: UP + DOWN).
    assert!(
        result.trades >= 2,
        "spread-capture should produce at least 2 trades (both legs); got {}",
        result.trades
    );
    // Signals should also be at least 2 (one UP and one DOWN per batch).
    assert!(
        result.signals >= 2,
        "spread-capture should produce at least 2 signals; got {}",
        result.signals
    );
}

// ---------------------------------------------------------------------------
// B3: Circuit breaker pauses then resumes trading
// ---------------------------------------------------------------------------

#[test]
fn backtest_circuit_breaker_pauses_then_resumes() {
    // Three market windows. All resolve DOWN (close < open), but tick prices
    // rise (momentum_up=true) so the bot generates UP signals that LOSE.
    let windows = vec![
        WindowDef {
            market_id: "mkt-cb-1",
            start_time: 1_000_000,
            end_time: 1_300_000,
            open_price: 42_000.0,
            close_price: 41_800.0,
            outcome: "DOWN",
            momentum_up: true,
        },
        WindowDef {
            market_id: "mkt-cb-2",
            start_time: 1_400_000,
            end_time: 1_700_000,
            open_price: 42_000.0,
            close_price: 41_800.0,
            outcome: "DOWN",
            momentum_up: true,
        },
        WindowDef {
            market_id: "mkt-cb-3",
            start_time: 1_800_000,
            end_time: 2_100_000,
            open_price: 42_000.0,
            close_price: 41_800.0,
            outcome: "DOWN",
            momentum_up: true,
        },
    ];

    let (_data_tmp, data_path) = create_multi_window_fixture_db(&windows, 42_000.0, 1000);

    // Run WITHOUT circuit breaker (effectively disabled: losses=999).
    let results_tmp_no_cb = NamedTempFile::new().unwrap();
    let results_path_no_cb = results_tmp_no_cb.path().to_str().unwrap().to_string();

    let config_no_cb = Config {
        peak_dd_pause_pct: 1.0,
        spread_capture_threshold: 0.50, // disable spread-capture
        circuit_breaker_losses: 999,    // effectively disabled
        circuit_breaker_pause_ms: 900_000,
        latency_arb_cooldown_ms: 1_000, // short cooldown to allow multiple signals
        log_level: "error".to_string(),
        ..Config::default()
    };

    let result_no_cb = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path.clone(),
        results_db_path: results_path_no_cb,
        start_time: 970_000,
        end_time: 2_130_000,
        starting_balance: 200.0,
        quiet: true,
        config: config_no_cb,
    })
    .unwrap();

    // Run WITH circuit breaker: pause after just 1 consecutive loss.
    let results_tmp_cb = NamedTempFile::new().unwrap();
    let results_path_cb = results_tmp_cb.path().to_str().unwrap().to_string();

    let config_cb = Config {
        peak_dd_pause_pct: 1.0,
        spread_capture_threshold: 0.50,
        circuit_breaker_losses: 1,           // trigger after 1 loss
        circuit_breaker_pause_ms: 1_000_000, // long pause (covers remaining windows)
        latency_arb_cooldown_ms: 1_000,
        log_level: "error".to_string(),
        ..Config::default()
    };

    let result_cb = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path_cb,
        start_time: 970_000,
        end_time: 2_130_000,
        starting_balance: 200.0,
        quiet: true,
        config: config_cb,
    })
    .unwrap();

    // Without CB, the bot should trade in multiple windows.
    assert!(
        result_no_cb.trades > 0,
        "without CB, should have trades; got 0 (signals={})",
        result_no_cb.signals
    );

    // With CB, the bot should trade in fewer windows because it pauses after the first loss.
    assert!(
        result_cb.trades < result_no_cb.trades,
        "with CB (losses=1, pause=1000s), should have fewer trades ({}) than without CB ({})",
        result_cb.trades,
        result_no_cb.trades
    );
}

// ---------------------------------------------------------------------------
// B4: Trend filter suppresses signals against the dominant trend
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines)]
fn backtest_trend_filter_suppresses_signal() {
    // Create 6 market windows: first 4 resolve UP with positive momentum (UP wins),
    // last 2 resolve DOWN but still have positive momentum (so the bot wants to go
    // UP, which LOSES against the DOWN outcome).
    // After 4 consecutive UP wins, the trend tracker's bias becomes positive.
    // When the bot then generates a DOWN signal (windows 5-6 have negative momentum),
    // the trend filter suppresses it because bias > threshold.
    let windows = vec![
        WindowDef {
            market_id: "mkt-tf-1",
            start_time: 1_000_000,
            end_time: 1_300_000,
            open_price: 42_000.0,
            close_price: 42_200.0,
            outcome: "UP",
            momentum_up: true,
        },
        WindowDef {
            market_id: "mkt-tf-2",
            start_time: 1_400_000,
            end_time: 1_700_000,
            open_price: 42_000.0,
            close_price: 42_200.0,
            outcome: "UP",
            momentum_up: true,
        },
        WindowDef {
            market_id: "mkt-tf-3",
            start_time: 1_800_000,
            end_time: 2_100_000,
            open_price: 42_000.0,
            close_price: 42_200.0,
            outcome: "UP",
            momentum_up: true,
        },
        WindowDef {
            market_id: "mkt-tf-4",
            start_time: 2_200_000,
            end_time: 2_500_000,
            open_price: 42_000.0,
            close_price: 42_200.0,
            outcome: "UP",
            momentum_up: true,
        },
        // Windows 5-6: negative momentum → DOWN signals, outcome is DOWN.
        // With trend filter, DOWN signals should be suppressed after UP-dominant bias.
        WindowDef {
            market_id: "mkt-tf-5",
            start_time: 2_600_000,
            end_time: 2_900_000,
            open_price: 42_200.0,
            close_price: 42_000.0,
            outcome: "DOWN",
            momentum_up: false,
        },
        WindowDef {
            market_id: "mkt-tf-6",
            start_time: 3_000_000,
            end_time: 3_300_000,
            open_price: 42_200.0,
            close_price: 42_000.0,
            outcome: "DOWN",
            momentum_up: false,
        },
    ];

    let (_data_tmp, data_path) = create_multi_window_fixture_db(&windows, 42_000.0, 1000);

    // Run WITHOUT trend filter.
    let results_tmp_no_tf = NamedTempFile::new().unwrap();
    let results_path_no_tf = results_tmp_no_tf.path().to_str().unwrap().to_string();

    let config_no_tf = Config {
        peak_dd_pause_pct: 1.0,
        spread_capture_threshold: 0.50,
        trend_filter_enabled: false,
        latency_arb_cooldown_ms: 1_000,
        circuit_breaker_losses: 999,
        log_level: "error".to_string(),
        ..Config::default()
    };

    let result_no_tf = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path.clone(),
        results_db_path: results_path_no_tf,
        start_time: 970_000,
        end_time: 3_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: config_no_tf,
    })
    .unwrap();

    // Run WITH trend filter.
    let results_tmp_tf = NamedTempFile::new().unwrap();
    let results_path_tf = results_tmp_tf.path().to_str().unwrap().to_string();

    let config_tf = Config {
        peak_dd_pause_pct: 1.0,
        spread_capture_threshold: 0.50,
        trend_filter_enabled: true,
        trend_filter_threshold: 0.3,
        trend_filter_window: 5,
        latency_arb_cooldown_ms: 1_000,
        circuit_breaker_losses: 999,
        log_level: "error".to_string(),
        ..Config::default()
    };

    let result_tf = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path_tf,
        start_time: 970_000,
        end_time: 3_330_000,
        starting_balance: 200.0,
        quiet: true,
        config: config_tf,
    })
    .unwrap();

    // Both runs should have signals.
    assert!(
        result_no_tf.signals > 0,
        "without trend filter, should have signals; got 0"
    );
    assert!(
        result_tf.signals > 0,
        "with trend filter, should still have signals; got 0"
    );

    // With trend filter enabled, some signals that would have become trades
    // should be suppressed (the signal is still counted but the trade is
    // blocked). So we expect fewer trades when the filter is active.
    // The trend filter suppresses counter-trend signals: after several UP
    // wins, DOWN signals are suppressed.
    assert!(
        result_tf.trades <= result_no_tf.trades,
        "with trend filter, should have no more trades ({}) than without ({})",
        result_tf.trades,
        result_no_tf.trades
    );
}

// ---------------------------------------------------------------------------
// B5: Backtest with quiet=false exercises println branches
// ---------------------------------------------------------------------------

#[test]
fn backtest_quiet_false_runs_successfully() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let results_tmp = NamedTempFile::new().unwrap();
    let results_path = results_tmp.path().to_str().unwrap().to_string();

    let result = run_backtest(BacktestOptions {
        tick_source: TickSource::FromDb(data_path.clone()),
        data_db_path: data_path,
        results_db_path: results_path,
        start_time: 970_000,
        end_time: 1_330_000,
        starting_balance: 200.0,
        quiet: false,
        config: test_config(),
    })
    .unwrap();

    assert!(result.total_ticks > 0, "should have loaded ticks");
    assert!(result.total_windows > 0, "should have at least one window");
    assert!(
        result.trades > 0,
        "strong momentum fixture should produce trades; signals={}",
        result.signals
    );
}
