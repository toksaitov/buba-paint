use clap::Parser;
use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

use buba_paint::cli::Cli;

/// Create a fixture data DB with the merged-data schema and a single 5-minute
/// market window where BTC goes **UP** (close > open).  Ticks simulate strong
/// upward momentum so the latency-arb strategy should trigger.
///
/// Timestamps: warmup at `970_000`ms, window `1_000_000..1_300_000`ms.
#[allow(clippy::too_many_lines)]
fn create_fixture_data_db() -> (NamedTempFile, String) {
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
        CREATE TABLE feed_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            received_at_ms INTEGER NOT NULL,
            received_at_us INTEGER,
            event_at_ms INTEGER,
            source TEXT NOT NULL,
            event_type TEXT NOT NULL,
            market_id TEXT,
            asset_id TEXT,
            price REAL,
            best_bid REAL,
            best_ask REAL,
            bid_size REAL,
            ask_size REAL,
            trade_size REAL,
            signed_quantity REAL,
            depth_bid_notional REAL,
            depth_ask_notional REAL,
            depth_imbalance REAL,
            microprice REAL,
            fidelity TEXT NOT NULL DEFAULT 'raw_event'
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
            "mkt-fixture-1",
            "Will BTC go up?",
            "cond-1",
            "btc-updown-5m-1000",
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
        insert_replay_grade_events(&conn, ts, base_price);
    }

    for i in 0..330 {
        let ts = 1_000_000_i64 + i64::from(i) * 1000;
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
        insert_replay_grade_events(&conn, ts, price);
    }

    drop(conn);
    (tmp, path)
}

/// Insert replay-grade feed events for one fixture timestamp.
fn insert_replay_grade_events(conn: &Connection, timestamp: i64, price: f64) {
    let timestamp_us = timestamp * 1_000;
    conn.execute(
        "INSERT INTO feed_events
         (received_at_ms, received_at_us, event_at_ms, source, event_type, price, trade_size, signed_quantity, fidelity)
         VALUES (?1, ?2, ?1, 'binance', 'aggTrade', ?3, 0.1, 0.1, 'raw_event')",
        params![timestamp, timestamp_us, price],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO feed_events
         (received_at_ms, received_at_us, event_at_ms, source, event_type, best_bid, best_ask, bid_size, ask_size, fidelity)
         VALUES (?1, ?2, ?1, 'binance', 'bookTicker', ?3, ?4, 1.0, 1.0, 'raw_event')",
        params![timestamp, timestamp_us + 1, price - 1.0, price + 1.0],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO feed_events
         (received_at_ms, received_at_us, event_at_ms, source, event_type, best_bid, best_ask, bid_size, ask_size, depth_bid_notional, depth_ask_notional, depth_imbalance, fidelity)
         VALUES (?1, ?2, ?1, 'binance', 'depth', ?3, ?4, 1.0, 1.0, 10000.0, 9000.0, 0.0526, 'raw_event')",
        params![timestamp, timestamp_us + 2, price - 1.0, price + 1.0],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO feed_events
         (received_at_ms, received_at_us, event_at_ms, source, event_type, price, fidelity)
         VALUES (?1, ?2, ?1, 'chainlink', 'chainlink_price', ?3, 'raw_event')",
        params![timestamp, timestamp_us + 3, price],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO feed_events
         (received_at_ms, received_at_us, event_at_ms, source, event_type, market_id, asset_id, best_bid, best_ask, bid_size, ask_size, fidelity)
         VALUES (?1, ?2, ?1, 'clob_up', 'best_bid_ask', 'mkt-fixture-1', 'tok-up', 0.40, 0.45, 100.0, 100.0, 'raw_event')",
        params![timestamp, timestamp_us + 4],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO feed_events
         (received_at_ms, received_at_us, event_at_ms, source, event_type, market_id, asset_id, best_bid, best_ask, bid_size, ask_size, fidelity)
         VALUES (?1, ?2, ?1, 'clob_down', 'best_bid_ask', 'mkt-fixture-1', 'tok-down', 0.40, 0.50, 100.0, 100.0, 'raw_event')",
        params![timestamp, timestamp_us + 5],
    )
    .unwrap();
}

/// Insert tick.
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

/// Verify that `cli::run()` with the `backtest` subcommand executes
/// end-to-end and produces a populated output database.
#[tokio::test]
async fn cli_run_backtest_with_fixture_data() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let output_tmp = NamedTempFile::new().unwrap();
    let output_path = output_tmp.path().to_str().unwrap().to_string();

    let cli = Cli::parse_from([
        "buba-paint",
        "backtest",
        "--data",
        &data_path,
        "--start",
        "1970-01-01T00:16:10Z",
        "--end",
        "1970-01-01T00:22:10Z",
        "--output",
        &output_path,
        "--balance",
        "200",
        "--set",
        "PEAK_DD_PAUSE_PCT=1.0",
        "--set",
        "SPREAD_CAPTURE_THRESHOLD=0.50",
    ]);

    let result = buba_paint::cli::run(cli).await;
    assert!(result.is_ok(), "cli::run(backtest) failed: {result:?}");

    let conn =
        Connection::open_with_flags(&output_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();

    let balance_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM balance_log", [], |r| r.get(0))
        .unwrap();
    assert!(
        balance_count > 0,
        "output DB balance_log should have at least one entry; got {balance_count}",
    );

    let market_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert!(
        market_count > 0,
        "output DB markets should have at least one entry; got {market_count}",
    );
}

/// Verify that `cli::run()` with the `sweep` subcommand executes
/// end-to-end and produces a CSV output file with correct structure.
#[tokio::test]
async fn cli_run_sweep_with_fixture_data() {
    for i in 0..10 {
        for suffix in ["", "-shm", "-wal"] {
            let _ = std::fs::remove_file(format!("/tmp/buba-sweep-{i:04}.db{suffix}"));
        }
    }

    let (_data_tmp, data_path) = create_fixture_data_db();
    let csv_tmp = NamedTempFile::new().unwrap();
    let csv_path = csv_tmp.path().to_str().unwrap().to_string();

    let cli = Cli::parse_from([
        "buba-paint",
        "sweep",
        "--data",
        &data_path,
        "--start",
        "1970-01-01T00:16:10Z",
        "--end",
        "1970-01-01T00:22:10Z",
        "--output",
        &csv_path,
        "--balance",
        "200",
        "--sweep",
        "LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.002:0.001",
        "--set",
        "PEAK_DD_PAUSE_PCT=1.0",
        "--set",
        "SPREAD_CAPTURE_THRESHOLD=0.50",
    ]);

    let result = buba_paint::cli::run(cli).await;
    assert!(result.is_ok(), "cli::run(sweep) failed: {result:?}");

    let csv_content = std::fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = csv_content.lines().collect();

    assert!(
        lines.len() >= 3,
        "sweep CSV should have header + at least 2 data rows; got {} lines",
        lines.len(),
    );

    let header = lines[0];
    assert!(
        header.contains("LATENCY_ARB_MOMENTUM_THRESHOLD"),
        "header missing sweep param column",
    );
    for col in ["pnl", "trades", "win_rate", "final_balance"] {
        assert!(
            header.contains(col),
            "header missing expected column '{col}'",
        );
    }

    let header_col_count = header.split(',').count();
    for (i, line) in lines.iter().skip(1).enumerate() {
        let col_count = line.split(',').count();
        assert_eq!(
            col_count, header_col_count,
            "row {i} has {col_count} columns, expected {header_col_count}",
        );
    }
}
