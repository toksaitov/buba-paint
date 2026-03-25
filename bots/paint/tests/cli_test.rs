// Integration tests for `buba_paint::cli::run()`.
//
// Verifies that the CLI dispatch layer correctly wires up the backtest and
// sweep subcommands end-to-end, using fixture data in temporary databases.

use clap::Parser;
use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

use buba_paint::cli::Cli;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

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

    // 30 seconds of warmup ticks before the window opens.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that `cli::run()` with the `backtest` subcommand executes
/// end-to-end and produces a populated output database.
#[tokio::test]
async fn cli_run_backtest_with_fixture_data() {
    let (_data_tmp, data_path) = create_fixture_data_db();
    let output_tmp = NamedTempFile::new().unwrap();
    let output_path = output_tmp.path().to_str().unwrap().to_string();

    // Timestamps:
    //   970_000ms  = 1970-01-01T00:16:10Z
    //   1_330_000ms = 1970-01-01T00:22:10Z
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

    // Verify the output DB exists and has data.
    let conn =
        Connection::open_with_flags(&output_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();

    // The balance_log table should have at least the initial "init" entry.
    let balance_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM balance_log", [], |r| r.get(0))
        .unwrap();
    assert!(
        balance_count > 0,
        "output DB balance_log should have at least one entry; got {balance_count}",
    );

    // The markets table should have been populated.
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
    // Clean up any stale temp DBs from previous sweep runs.
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

    // Verify the CSV output file exists and has content.
    let csv_content = std::fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = csv_content.lines().collect();

    // Should have at least a header + 2 data rows (sweep range 0.001..0.002 step 0.001).
    assert!(
        lines.len() >= 3,
        "sweep CSV should have header + at least 2 data rows; got {} lines",
        lines.len(),
    );

    // Header should contain expected column names.
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

    // All data rows should have the same number of columns as the header.
    let header_col_count = header.split(',').count();
    for (i, line) in lines.iter().skip(1).enumerate() {
        let col_count = line.split(',').count();
        assert_eq!(
            col_count, header_col_count,
            "row {i} has {col_count} columns, expected {header_col_count}",
        );
    }
}
