use super::*;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

type MarketTuple<'a> = (
    &'a str, // market_id
    &'a str, // question
    &'a str, // condition_id
    &'a str, // slug
    &'a str, // up_token_id
    &'a str, // down_token_id
    i64,     // start_time
    i64,     // end_time
);

type TradeTuple<'a> = (
    i64,     // timestamp
    &'a str, // strategy
    &'a str, // side
    f64,     // entry_price
    f64,     // size
    f64,     // settlement_price
    f64,     // pnl_0pct
);

/// Create a minimal source DB mimicking a run's schema.
fn create_fixture_run_db(
    path: &str,
    ticks: &[(i64, &str, f64)],
    markets: &[MarketTuple<'_>],
    trades: &[TradeTuple<'_>],
) {
    let conn = Connection::open(path).unwrap();
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
            status        TEXT NOT NULL DEFAULT 'active'
        );
        CREATE TABLE simulated_trades (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   INTEGER NOT NULL,
            market_id   TEXT NOT NULL,
            strategy    TEXT NOT NULL,
            side        TEXT NOT NULL,
            token_id    TEXT NOT NULL,
            entry_price REAL NOT NULL,
            size        REAL NOT NULL,
            status      TEXT NOT NULL DEFAULT 'open'
        );
        CREATE TABLE trade_results (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            trade_id         INTEGER NOT NULL UNIQUE,
            exit_price       REAL,
            settlement_price REAL NOT NULL,
            pnl_0pct         REAL NOT NULL,
            pnl_1pct         REAL NOT NULL,
            pnl_2pct         REAL NOT NULL,
            pnl_3pct         REAL NOT NULL,
            resolved_at      INTEGER NOT NULL
        );",
    )
    .unwrap();

    for (ts, source, price) in ticks {
        conn.execute(
            "INSERT INTO tick_data (timestamp, source, price) VALUES (?1, ?2, ?3)",
            params![ts, source, price],
        )
        .unwrap();
    }

    for (mid, q, cid, slug, up, down, st, et) in markets {
        conn.execute(
            "INSERT INTO markets (market_id, question, condition_id, slug,
                up_token_id, down_token_id, start_time, end_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![mid, q, cid, slug, up, down, st, et],
        )
        .unwrap();
    }

    for (ts, strategy, side, entry_price, size, settlement_price, pnl) in trades {
        conn.execute(
            "INSERT INTO simulated_trades (timestamp, market_id, strategy, side,
                token_id, entry_price, size, status)
             VALUES (?1, 'mkt-1', ?2, ?3, 'tok-up', ?4, ?5, 'closed')",
            params![ts, strategy, side, entry_price, size],
        )
        .unwrap();

        let trade_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO trade_results (trade_id, exit_price, settlement_price,
                pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at)
             VALUES (?1, 0.60, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                trade_id,
                settlement_price,
                pnl,
                pnl - 0.5,
                pnl - 1.0,
                pnl - 1.5,
                *ts + 300_000
            ],
        )
        .unwrap();
    }
}

type RunConfig<'a> = (
    &'a str,
    &'a [(i64, &'a str, f64)],
    &'a [MarketTuple<'a>],
    &'a [TradeTuple<'a>],
);

/// Set up a temp directory with run subdirectories containing fixture DBs.
/// Returns (`TempDir`, runs dir path, output path).
fn setup_fixture_runs(run_configs: &[RunConfig<'_>]) -> (TempDir, String, String) {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("runs");
    std::fs::create_dir_all(&base).unwrap();
    let output_path = dir.path().join("output").join("market-data.db");

    for (subdir, ticks, markets, trades) in run_configs {
        let sub = base.join(subdir);
        std::fs::create_dir_all(&sub).unwrap();
        let db_path = sub.join("buba-paint.db");
        create_fixture_run_db(db_path.to_str().unwrap(), ticks, markets, trades);
    }

    (
        dir,
        base.to_str().unwrap().to_string(),
        output_path.to_str().unwrap().to_string(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn creates_schema_with_all_tables() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("market-data.db");
    build_market_data("nonexistent_dir", output.to_str().unwrap()).unwrap();
    let db = Connection::open(&output).unwrap();
    for table in [
        "runs",
        "tick_data",
        "markets",
        "data_quality",
        "historical_trades",
    ] {
        let count: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "table {table} should exist and be empty");
    }
}

#[test]
fn imports_tick_data_from_source_db() {
    let ticks: Vec<(i64, &str, f64)> = (0..100)
        .map(|i| (1_700_000_000_000 + i * 1000, "binance", 42_000.0 + i as f64))
        .collect();

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &[], &[])]);

    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM tick_data WHERE run_id = 4", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 100, "should import all 100 ticks");
}

#[test]
fn imports_markets_with_dedup() {
    let shared_market = (
        "mkt-shared",
        "Will BTC go up?",
        "cond-1",
        "btc-up",
        "tok-up",
        "tok-down",
        1_000_000_i64,
        1_300_000_i64,
    );
    let unique_market = (
        "mkt-unique",
        "Will BTC go down?",
        "cond-2",
        "btc-down",
        "tok-up-2",
        "tok-down-2",
        1_300_000_i64,
        1_600_000_i64,
    );

    let (_dir, runs_dir, output) = setup_fixture_runs(&[
        (
            "004",
            &[(1_000_000, "binance", 42_000.0)],
            &[shared_market, unique_market],
            &[],
        ),
        (
            "005",
            &[(1_100_000, "binance", 42_100.0)],
            &[shared_market], // duplicate
            &[],
        ),
    ]);

    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    // INSERT OR IGNORE: shared_market inserted once, unique_market once = 2
    assert_eq!(count, 2, "should deduplicate markets by market_id");
}

#[test]
fn imports_historical_trades() {
    let trades = vec![
        (1_000_000_i64, "latency-arb", "UP", 0.52, 50.0, 1.0, 24.0),
        (1_100_000, "spread-capture", "DOWN", 0.48, 30.0, 0.0, -14.4),
    ];

    let (_dir, runs_dir, output) =
        setup_fixture_runs(&[("004", &[(1_000_000, "binance", 42_000.0)], &[], &trades)]);

    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM historical_trades WHERE run_id = 4",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2, "should import both trades");

    // Verify the "won" field: first trade has pnl > 0, second has pnl < 0
    let won_count: i64 = db
        .query_row(
            "SELECT SUM(won) FROM historical_trades WHERE run_id = 4",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(won_count, 1, "should mark exactly one trade as won");
}

#[test]
fn computes_settlements_correctly() {
    // Market window: 1_000_000 to 1_300_000
    // Chainlink tick at 999_000 (before start): price = 42_000 -> open_price
    // Chainlink tick at 1_200_000 (before end):  price = 42_100 -> close_price
    // close >= open -> outcome = UP
    let ticks = vec![
        (999_000_i64, "chainlink", 42_000.0),
        (1_200_000, "chainlink", 42_100.0),
    ];
    let markets = vec![(
        "mkt-1",
        "Will BTC go up?",
        "cond-1",
        "btc-up",
        "tok-up",
        "tok-down",
        1_000_000_i64,
        1_300_000_i64,
    )];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &markets, &[])]);

    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    let (open_price, close_price, outcome): (f64, f64, String) = db
        .query_row(
            "SELECT open_price, close_price, outcome FROM markets WHERE market_id = 'mkt-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    assert!(
        (open_price - 42_000.0).abs() < f64::EPSILON,
        "open_price should be 42000, got {open_price}"
    );
    assert!(
        (close_price - 42_100.0).abs() < f64::EPSILON,
        "close_price should be 42100, got {close_price}"
    );
    assert_eq!(outcome, "UP");
}

#[test]
fn settlement_down_when_close_below_open() {
    let ticks = vec![
        (999_000_i64, "chainlink", 42_100.0),
        (1_200_000, "chainlink", 42_000.0),
    ];
    let markets = vec![(
        "mkt-1",
        "Will BTC go up?",
        "cond-1",
        "btc-up",
        "tok-up",
        "tok-down",
        1_000_000_i64,
        1_300_000_i64,
    )];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &markets, &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let outcome: String = db
        .query_row(
            "SELECT outcome FROM markets WHERE market_id = 'mkt-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "DOWN");
}

#[test]
fn settlement_up_when_close_equals_open() {
    let ticks = vec![
        (999_000_i64, "chainlink", 42_000.0),
        (1_200_000, "chainlink", 42_000.0),
    ];
    let markets = vec![(
        "mkt-1",
        "Will BTC go up?",
        "cond-1",
        "btc-up",
        "tok-up",
        "tok-down",
        1_000_000_i64,
        1_300_000_i64,
    )];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &markets, &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let outcome: String = db
        .query_row(
            "SELECT outcome FROM markets WHERE market_id = 'mkt-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "UP", "close == open should be UP");
}

#[test]
fn skips_missing_run_db() {
    let dir = TempDir::new().unwrap();
    let runs_dir = dir.path().join("runs");
    // Don't create any run subdirectories
    std::fs::create_dir_all(&runs_dir).unwrap();
    let output = dir.path().join("market-data.db");

    // Should not crash, just skip all missing runs
    let result = build_market_data(runs_dir.to_str().unwrap(), output.to_str().unwrap());
    assert!(result.is_ok(), "should succeed even with no run DBs found");

    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "no runs should be imported");
}

#[test]
fn source_dbs_opened_read_only() {
    let ticks = vec![(1_000_000_i64, "binance", 42_000.0)];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &[], &[])]);

    // Hash the source file before
    let src_path = std::path::Path::new(&runs_dir)
        .join("004")
        .join("buba-paint.db");
    let before = std::fs::read(&src_path).unwrap();

    build_market_data(&runs_dir, &output).unwrap();

    // Hash the source file after
    let after = std::fs::read(&src_path).unwrap();
    assert_eq!(before, after, "source DB should not be modified");
}

#[test]
fn updates_run_stats() {
    let ticks = vec![
        (1_000_000_i64, "binance", 42_000.0),
        (1_200_000, "binance", 42_100.0),
    ];
    let trades = vec![
        (1_000_000_i64, "latency-arb", "UP", 0.52, 50.0, 1.0, 24.0),
        (1_100_000, "latency-arb", "DOWN", 0.48, 30.0, 0.0, -14.4),
        (1_200_000, "latency-arb", "UP", 0.50, 40.0, 1.0, 20.0),
    ];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &[], &trades)]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let (start_time, end_time, total_trades, win_rate): (i64, i64, i64, f64) = db
        .query_row(
            "SELECT start_time, end_time, total_trades, win_rate FROM runs WHERE run_number = 4",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();

    assert_eq!(start_time, 1_000_000);
    assert_eq!(end_time, 1_200_000);
    assert_eq!(total_trades, 3);
    // 2 wins out of 3 trades
    assert!((win_rate - 2.0 / 3.0).abs() < 0.001);
}

#[test]
fn creates_indexes() {
    let (_dir, runs_dir, output) =
        setup_fixture_runs(&[("004", &[(1_000_000, "binance", 42_000.0)], &[], &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    let expected_indexes = [
        "idx_tick_ts",
        "idx_tick_source_ts",
        "idx_tick_run",
        "idx_markets_start",
        "idx_markets_end",
        "idx_markets_outcome",
        "idx_markets_run",
        "idx_htrades_run",
    ];

    for idx in expected_indexes {
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1",
                params![idx],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "index {idx} should exist");
    }
}

#[test]
fn computes_data_quality() {
    // Create ticks spanning 2 hours with a known gap pattern.
    // Hour boundary: 0..3_600_000 and 3_600_000..7_200_000
    let mut ticks: Vec<(i64, &str, f64)> = Vec::new();

    // First hour: tick every second for first 100 seconds, then a 10s gap, then more
    for i in 0..100 {
        ticks.push((i * 1000, "binance", 42_000.0));
    }
    // Gap from 100s to 110s (10s gap -> counted as >5s but not >30s)
    for i in 110..200 {
        ticks.push((i * 1000, "binance", 42_000.0));
    }

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &[], &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM data_quality", [], |r| r.get(0))
        .unwrap();
    assert!(count > 0, "data_quality should have entries");

    // Check the specific hour that has our data
    let (tick_count, gap_5s, gap_30s): (i64, i64, i64) = db
        .query_row(
            "SELECT tick_count, gap_count_5s, gap_count_30s FROM data_quality
             WHERE source = 'binance' AND hour_start = 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(tick_count, 190, "should have 190 ticks in first hour");
    assert_eq!(gap_5s, 1, "should detect 1 gap >5s");
    assert_eq!(gap_30s, 0, "should detect 0 gaps >30s");
}

#[test]
fn output_db_uses_wal_mode() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("market-data.db");
    build_market_data("nonexistent_dir", output.to_str().unwrap()).unwrap();

    let db = Connection::open(&output).unwrap();
    let mode: String = db
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn multiple_runs_imported() {
    let (_dir, runs_dir, output) = setup_fixture_runs(&[
        (
            "004",
            &[(1_000_000, "binance", 42_000.0)],
            &[],
            &[(1_000_000, "latency-arb", "UP", 0.52, 50.0, 1.0, 24.0)],
        ),
        (
            "005",
            &[(2_000_000, "binance", 43_000.0)],
            &[],
            &[(2_000_000, "spread-capture", "DOWN", 0.48, 30.0, 0.0, -14.4)],
        ),
    ]);

    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    let run_count: i64 = db
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(run_count, 2);

    let tick_count: i64 = db
        .query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tick_count, 2);

    let trade_count: i64 = db
        .query_row("SELECT COUNT(*) FROM historical_trades", [], |r| r.get(0))
        .unwrap();
    assert_eq!(trade_count, 2);
}

#[test]
fn deletes_existing_output_before_rebuild() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("market-data.db");

    // Create a dummy file at the output path
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(&output, b"dummy data").unwrap();

    build_market_data("nonexistent_dir", output.to_str().unwrap()).unwrap();

    // Output should now be a valid SQLite DB, not the dummy data
    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn creates_output_parent_directories() {
    let dir = TempDir::new().unwrap();
    let output = dir
        .path()
        .join("deeply")
        .join("nested")
        .join("dir")
        .join("market-data.db");

    build_market_data("nonexistent_dir", output.to_str().unwrap()).unwrap();
    assert!(output.exists(), "should create parent dirs for output");
}

#[test]
fn run_versions_stored_correctly() {
    let (_dir, runs_dir, output) = setup_fixture_runs(&[
        ("004", &[(1_000_000, "binance", 42_000.0)], &[], &[]),
        ("005", &[(2_000_000, "binance", 43_000.0)], &[], &[]),
    ]);

    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    let mut versions: HashMap<i64, String> = HashMap::new();
    let mut stmt = db
        .prepare("SELECT run_number, bot_version FROM runs ORDER BY run_number")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .unwrap();
    for row in rows {
        let (num, ver) = row.unwrap();
        versions.insert(num, ver);
    }

    assert_eq!(versions.get(&4).map(String::as_str), Some("v0.2"));
    assert_eq!(versions.get(&5).map(String::as_str), Some("v0.3"));
}

// ---------------------------------------------------------------------------
// Additional coverage tests
// ---------------------------------------------------------------------------

#[test]
fn settlement_uses_correct_chainlink_prices() {
    // 3 markets at different time windows.
    // Chainlink ticks placed so each market picks up the correct open/close.
    //
    // The correlated subquery is:
    //   open_price  = latest chainlink tick WHERE timestamp <= start_time
    //   close_price = latest chainlink tick WHERE timestamp <= end_time
    //
    // Chainlink ticks:
    //   t=999_000  -> $40_000    (before market 1 start)
    //   t=1_200_000 -> $41_000   (between market 1 start and end)
    //   t=1_500_000 -> $42_000   (between market 2 start and end)
    //   t=1_900_000 -> $39_000   (between market 3 start and end -- but before close tick)
    //   t=2_050_000 -> $43_000   (between market 3 start and end -- latest before end)
    //
    // Market 1: start=1_000_000, end=1_300_000
    //   open  = tick at 999_000 = $40_000
    //   close = tick at 1_200_000 = $41_000  (latest <= 1_300_000)
    //   outcome = UP (41000 >= 40000)
    //
    // Market 2: start=1_400_000, end=1_700_000
    //   open  = tick at 1_200_000 = $41_000  (latest <= 1_400_000)
    //   close = tick at 1_500_000 = $42_000  (latest <= 1_700_000)
    //   outcome = UP (42000 >= 41000)
    //
    // Market 3: start=1_800_000, end=2_100_000
    //   open  = tick at 1_500_000 = $42_000  (latest <= 1_800_000)
    //   close = tick at 2_050_000 = $43_000  (latest <= 2_100_000)
    //   outcome = UP (43000 >= 42000)

    let ticks = vec![
        (999_000_i64, "chainlink", 40_000.0),
        (1_200_000, "chainlink", 41_000.0),
        (1_500_000, "chainlink", 42_000.0),
        (1_900_000, "chainlink", 39_000.0),
        (2_050_000, "chainlink", 43_000.0),
    ];
    let markets = vec![
        (
            "mkt-1",
            "BTC 5min #1",
            "cond-1",
            "btc-updown-1",
            "tok-up-1",
            "tok-down-1",
            1_000_000_i64,
            1_300_000_i64,
        ),
        (
            "mkt-2",
            "BTC 5min #2",
            "cond-2",
            "btc-updown-2",
            "tok-up-2",
            "tok-down-2",
            1_400_000_i64,
            1_700_000_i64,
        ),
        (
            "mkt-3",
            "BTC 5min #3",
            "cond-3",
            "btc-updown-3",
            "tok-up-3",
            "tok-down-3",
            1_800_000_i64,
            2_100_000_i64,
        ),
    ];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &markets, &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    // Verify each market's settlement
    let expected: &[(&str, f64, f64, &str)] = &[
        ("mkt-1", 40_000.0, 41_000.0, "UP"),
        ("mkt-2", 41_000.0, 42_000.0, "UP"),
        ("mkt-3", 42_000.0, 43_000.0, "UP"),
    ];

    for (market_id, exp_open, exp_close, exp_outcome) in expected {
        let (open_price, close_price, outcome): (f64, f64, String) = db
            .query_row(
                "SELECT open_price, close_price, outcome FROM markets WHERE market_id = ?1",
                params![market_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();

        assert!(
            (open_price - exp_open).abs() < f64::EPSILON,
            "{market_id}: open_price should be {exp_open}, got {open_price}"
        );
        assert!(
            (close_price - exp_close).abs() < f64::EPSILON,
            "{market_id}: close_price should be {exp_close}, got {close_price}"
        );
        assert_eq!(
            outcome, *exp_outcome,
            "{market_id}: outcome should be {exp_outcome}, got {outcome}"
        );
    }
}

#[test]
fn settlement_down_with_price_drop() {
    // Verifies DOWN outcome when chainlink price drops significantly.
    // 1 market: start=1_000_000, end=1_300_000
    // Chainlink at 999_000 -> $50_000 (open), at 1_200_000 -> $48_000 (close)
    // close < open -> DOWN
    let ticks = vec![
        (999_000_i64, "chainlink", 50_000.0),
        (1_200_000, "chainlink", 48_000.0),
    ];
    let markets = vec![(
        "mkt-1",
        "BTC 5min drop",
        "cond-1",
        "btc-updown-drop",
        "tok-up",
        "tok-down",
        1_000_000_i64,
        1_300_000_i64,
    )];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &markets, &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();
    let (open_price, close_price, outcome): (f64, f64, String) = db
        .query_row(
            "SELECT open_price, close_price, outcome FROM markets WHERE market_id = 'mkt-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    assert!(
        (open_price - 50_000.0).abs() < f64::EPSILON,
        "open_price should be 50000, got {open_price}"
    );
    assert!(
        (close_price - 48_000.0).abs() < f64::EPSILON,
        "close_price should be 48000, got {close_price}"
    );
    assert_eq!(outcome, "DOWN");
}

#[test]
fn data_quality_gap_detection() {
    // Source "binance", hour starting at 3_600_000 (1 hour after epoch).
    // Ticks at: 3_600_000, 3_601_000, 3_602_000, 3_613_000 (11s gap),
    //           3_614_000, 3_650_000 (36s gap), 3_651_000
    //
    // Gaps between consecutive ticks:
    //   3_601_000 - 3_600_000 = 1_000 ms (not a gap)
    //   3_602_000 - 3_601_000 = 1_000 ms (not a gap)
    //   3_613_000 - 3_602_000 = 11_000 ms (>5s gap, not >30s)
    //   3_614_000 - 3_613_000 = 1_000 ms (not a gap)
    //   3_650_000 - 3_614_000 = 36_000 ms (>5s AND >30s gap)
    //   3_651_000 - 3_650_000 = 1_000 ms (not a gap)
    //
    // Expected: tick_count=7, gap_count_5s=2, gap_count_30s=1,
    //           max_gap_ms=36000, coverage=7/3600 ~= 0.001944

    let ticks = vec![
        (3_600_000_i64, "binance", 42_000.0),
        (3_601_000, "binance", 42_001.0),
        (3_602_000, "binance", 42_002.0),
        (3_613_000, "binance", 42_003.0),
        (3_614_000, "binance", 42_004.0),
        (3_650_000, "binance", 42_005.0),
        (3_651_000, "binance", 42_006.0),
    ];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &[], &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    let (tick_count, gap_5s, gap_30s, max_gap, coverage): (i64, i64, i64, i64, f64) = db
        .query_row(
            "SELECT tick_count, gap_count_5s, gap_count_30s, max_gap_ms, coverage_pct
             FROM data_quality
             WHERE source = 'binance' AND hour_start = 3600000",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();

    assert_eq!(tick_count, 7, "should have 7 ticks");
    assert_eq!(gap_5s, 2, "should detect 2 gaps >5s");
    assert_eq!(gap_30s, 1, "should detect 1 gap >30s");
    assert_eq!(max_gap, 36_000, "max gap should be 36000ms");

    let expected_coverage = 7.0 / 3600.0;
    assert!(
        (coverage - expected_coverage).abs() < 0.0001,
        "coverage should be ~{expected_coverage:.4}, got {coverage:.4}"
    );
}

#[test]
fn data_quality_skips_empty_hours() {
    // Ticks in hour 0 (0..3_599_999) and hour 2 (7_200_000..10_799_999).
    // No ticks in hour 1 (3_600_000..7_199_999).
    // Verify data_quality has entries for hours 0 and 2 only.

    let ticks = vec![
        // Hour 0 ticks
        (500_000_i64, "binance", 42_000.0),
        (1_500_000, "binance", 42_001.0),
        (2_500_000, "binance", 42_002.0),
        // Hour 2 ticks (skip hour 1 entirely)
        (7_500_000_i64, "binance", 43_000.0),
        (8_500_000, "binance", 43_001.0),
    ];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &[], &[])]);
    build_market_data(&runs_dir, &output).unwrap();

    let db = Connection::open(&output).unwrap();

    // Collect all hour_start values for binance
    let mut stmt = db
        .prepare("SELECT hour_start FROM data_quality WHERE source = 'binance' ORDER BY hour_start")
        .unwrap();
    let hours: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<i64>, _>>()
        .unwrap();

    assert!(
        hours.contains(&0),
        "should have data_quality entry for hour 0"
    );
    assert!(
        !hours.contains(&3_600_000),
        "should NOT have data_quality entry for empty hour 1"
    );
    assert!(
        hours.contains(&7_200_000),
        "should have data_quality entry for hour 2"
    );
}

#[test]
fn print_summary_does_not_panic() {
    // Build a fixture with real data across all tables: runs, ticks, markets,
    // and trades. This exercises the print_summary code path end-to-end,
    // including the hours/date formatting, win_rate, data_quality stats.

    let ticks = vec![
        (1_000_000_i64, "chainlink", 42_000.0),
        (1_100_000, "chainlink", 42_050.0),
        (1_200_000, "chainlink", 42_100.0),
        (1_000_000, "binance", 42_001.0),
        (1_100_000, "binance", 42_051.0),
        (1_200_000, "binance", 42_101.0),
    ];
    let markets = vec![(
        "mkt-1",
        "BTC 5min summary",
        "cond-1",
        "btc-updown-summary",
        "tok-up",
        "tok-down",
        1_000_000_i64,
        1_300_000_i64,
    )];
    let trades = vec![
        (1_050_000_i64, "latency-arb", "UP", 0.52, 50.0, 1.0, 24.0),
        (1_150_000, "spread-capture", "DOWN", 0.48, 30.0, 0.0, -14.4),
    ];

    let (_dir, runs_dir, output) = setup_fixture_runs(&[("004", &ticks, &markets, &trades)]);

    // The main assertion: build_market_data (which calls print_summary) returns Ok.
    let result = build_market_data(&runs_dir, &output);
    assert!(
        result.is_ok(),
        "build_market_data should succeed without panic: {:?}",
        result.err()
    );

    // Verify print_summary had real data to work with.
    let db = Connection::open(&output).unwrap();
    let run_count: i64 = db
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert!(run_count > 0, "should have at least one run for summary");

    let tick_count: i64 = db
        .query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))
        .unwrap();
    assert!(tick_count > 0, "should have ticks for summary");

    let market_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM markets WHERE outcome IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(market_count > 0, "should have settled markets for summary");

    let trade_count: i64 = db
        .query_row("SELECT COUNT(*) FROM historical_trades", [], |r| r.get(0))
        .unwrap();
    assert!(trade_count > 0, "should have trades for summary");

    let quality_count: i64 = db
        .query_row("SELECT COUNT(*) FROM data_quality", [], |r| r.get(0))
        .unwrap();
    assert!(quality_count > 0, "should have data_quality for summary");
}

#[test]
fn build_data_creates_parent_dirs() {
    // Verify that deeply nested output paths have their parent directories
    // created automatically.
    let dir = TempDir::new().unwrap();
    let output = dir
        .path()
        .join("deep")
        .join("nested")
        .join("market-data.db");

    assert!(
        !output.parent().unwrap().exists(),
        "parent dirs should not exist yet"
    );

    build_market_data("nonexistent_dir", output.to_str().unwrap()).unwrap();

    assert!(output.exists(), "output DB should exist");
    assert!(
        output.parent().unwrap().exists(),
        "parent dirs should have been created"
    );

    // Verify the DB is functional
    let db = Connection::open(&output).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "DB should be valid with empty tables");
}
