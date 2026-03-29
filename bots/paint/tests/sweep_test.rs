// Integration tests for `buba_paint::backtest::sweep::run_sweep()`.
//
// Uses a temporary SQLite database pre-populated with fixture data (the
// merged-data schema) and verifies CSV output correctness and determinism.
//
// NOTE: Both tests are combined into a single function because the sweep
// engine uses shared temp paths (`/tmp/buba-sweep-NNNN.db`) that cannot
// be safely accessed from parallel test threads.

use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

use buba_paint::backtest::sweep::{SweepDimension, run_sweep};

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

    // 30 seconds of warmup ticks before the window.
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

    // During the window: rising prices + flat CLOB asks.
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

fn fixed_overrides() -> Vec<(String, String)> {
    vec![
        ("SPREAD_CAPTURE_THRESHOLD".to_string(), "0.50".to_string()),
        ("PEAK_DD_PAUSE_PCT".to_string(), "1.0".to_string()),
    ]
}

fn two_by_two_dimensions() -> Vec<SweepDimension> {
    vec![
        SweepDimension {
            param: "LATENCY_ARB_MOMENTUM_THRESHOLD".to_string(),
            values: vec![0.001, 0.002],
        },
        SweepDimension {
            param: "LATENCY_ARB_MAX_ASK".to_string(),
            values: vec![0.50, 0.65],
        },
    ]
}

/// Clean up shared temp DB files used by the sweep engine.
fn cleanup_sweep_temp_dbs() {
    for i in 0..10 {
        for suffix in ["", "-shm", "-wal"] {
            let _ = std::fs::remove_file(format!("/tmp/buba-sweep-{i:04}.db{suffix}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verifies sweep CSV output (correct structure, parseable values) and
/// determinism (two identical sweeps produce the same results).
#[test]
#[allow(clippy::too_many_lines)]
fn sweep_produces_correct_csv_and_is_deterministic() {
    cleanup_sweep_temp_dbs();
    let (_data_tmp, data_path) = create_fixture_data_db();

    // --- Part 1: CSV correctness ---

    let csv_tmp = NamedTempFile::new().unwrap();
    let csv_path = csv_tmp.path().to_str().unwrap().to_string();

    run_sweep(
        &data_path,
        &csv_path,
        970_000,
        1_330_000,
        200.0,
        &two_by_two_dimensions(),
        &fixed_overrides(),
    )
    .unwrap();

    let csv_content = std::fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = csv_content.lines().collect();

    // 1 header + 4 data rows (2x2 grid).
    assert_eq!(lines.len(), 5, "expected 5 lines, got {}", lines.len());

    let header = lines[0];
    assert!(header.contains("LATENCY_ARB_MOMENTUM_THRESHOLD"));
    assert!(header.contains("LATENCY_ARB_MAX_ASK"));
    for col in [
        "pnl",
        "win_rate",
        "trades",
        "wins",
        "losses",
        "max_dd",
        "hwm",
        "final_balance",
        "signals",
        "elapsed_s",
    ] {
        assert!(header.contains(col), "header missing column '{col}'");
    }

    let header_cols: Vec<&str> = header.split(',').collect();
    assert_eq!(
        header_cols.len(),
        14,
        "expected 14 columns, got {}",
        header_cols.len()
    );

    // All data values must be parseable as f64.
    for (row_idx, line) in lines.iter().skip(1).enumerate() {
        let cols: Vec<&str> = line.split(',').collect();
        assert_eq!(cols.len(), 14);
        for (col_idx, col) in cols.iter().enumerate() {
            col.parse::<f64>().unwrap_or_else(|e| {
                panic!(
                    "row {row_idx}, col {col_idx} ({}) not a number: '{col}' ({e})",
                    header_cols[col_idx]
                );
            });
        }
    }

    // --- Part 2: Determinism ---

    cleanup_sweep_temp_dbs();

    let csv_tmp_2 = NamedTempFile::new().unwrap();
    let csv_path_2 = csv_tmp_2.path().to_str().unwrap().to_string();

    run_sweep(
        &data_path,
        &csv_path_2,
        970_000,
        1_330_000,
        200.0,
        &two_by_two_dimensions(),
        &fixed_overrides(),
    )
    .unwrap();

    let csv2 = std::fs::read_to_string(&csv_path_2).unwrap();
    let lines2: Vec<&str> = csv2.lines().collect();

    assert_eq!(lines.len(), lines2.len());
    assert_eq!(lines[0], lines2[0], "headers differ");

    // Sort rows (rayon ordering may differ) and compare excluding elapsed_s.
    let mut data1: Vec<&str> = lines[1..].to_vec();
    let mut data2: Vec<&str> = lines2[1..].to_vec();
    data1.sort_unstable();
    data2.sort_unstable();

    let elapsed_col_idx = header_cols
        .iter()
        .position(|&c| c == "elapsed_s")
        .expect("elapsed_s column not found");

    for (row_idx, (r1, r2)) in data1.iter().zip(data2.iter()).enumerate() {
        let c1: Vec<&str> = r1.split(',').collect();
        let c2: Vec<&str> = r2.split(',').collect();
        for (col_idx, (v1, v2)) in c1.iter().zip(c2.iter()).enumerate() {
            if col_idx == elapsed_col_idx {
                continue;
            }
            assert_eq!(
                v1, v2,
                "row {row_idx}, col {col_idx} ({}) differs: '{v1}' vs '{v2}'",
                header_cols[col_idx]
            );
        }
    }
}
