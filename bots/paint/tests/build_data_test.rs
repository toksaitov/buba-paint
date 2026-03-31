use rusqlite::{Connection, params};
use tempfile::TempDir;

type MarketTuple<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    i64,
    i64,
);
type TradeTuple<'a> = (i64, &'a str, &'a str, f64, f64, f64, f64);

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

/// Verifies that build data end to end.
#[test]
#[allow(clippy::too_many_lines)]
fn build_data_end_to_end() {
    let dir = TempDir::new().unwrap();
    let runs_dir = dir.path().join("runs");
    std::fs::create_dir_all(&runs_dir).unwrap();

    let output_path = dir.path().join("data").join("market-data.db");

    let run_004_dir = runs_dir.join("004");
    std::fs::create_dir_all(&run_004_dir).unwrap();
    let run_004_db = run_004_dir.join("buba-paint.db");

    let markets_004 = vec![
        (
            "mkt-1",
            "Will BTC go up?",
            "cond-1",
            "btc-up-1",
            "tok-up-1",
            "tok-down-1",
            1_000_000_i64,
            1_300_000_i64,
        ),
        (
            "mkt-2",
            "Will BTC go up again?",
            "cond-2",
            "btc-up-2",
            "tok-up-2",
            "tok-down-2",
            1_300_000_i64,
            1_600_000_i64,
        ),
    ];

    let ticks_004: Vec<(i64, &str, f64)> = vec![
        (900_000, "chainlink", 42_000.0),
        (1_000_000, "binance", 42_000.0),
        (1_100_000, "chainlink", 42_050.0),
        (1_200_000, "binance", 42_100.0),
        (1_250_000, "chainlink", 42_100.0),
    ];

    let trades_004 = vec![
        (1_000_000_i64, "latency-arb", "UP", 0.52, 50.0, 1.0, 24.0),
        (1_100_000, "latency-arb", "DOWN", 0.48, 30.0, 0.0, -14.4),
    ];

    create_fixture_run_db(
        run_004_db.to_str().unwrap(),
        &ticks_004,
        &markets_004,
        &trades_004,
    );

    let run_005_dir = runs_dir.join("005");
    std::fs::create_dir_all(&run_005_dir).unwrap();
    let run_005_db = run_005_dir.join("buba-paint.db");

    let markets_005 = vec![
        (
            "mkt-1",
            "Will BTC go up?",
            "cond-1",
            "btc-up-1",
            "tok-up-1",
            "tok-down-1",
            1_000_000_i64,
            1_300_000_i64,
        ),
        (
            "mkt-3",
            "Will BTC go up third?",
            "cond-3",
            "btc-up-3",
            "tok-up-3",
            "tok-down-3",
            1_600_000_i64,
            1_900_000_i64,
        ),
    ];

    let ticks_005: Vec<(i64, &str, f64)> = vec![
        (2_000_000, "binance", 43_000.0),
        (2_100_000, "chainlink", 43_050.0),
        (2_200_000, "binance", 43_100.0),
    ];

    let trades_005 = vec![(
        2_000_000_i64,
        "spread-capture",
        "DOWN",
        0.48,
        30.0,
        0.0,
        -14.4,
    )];

    create_fixture_run_db(
        run_005_db.to_str().unwrap(),
        &ticks_005,
        &markets_005,
        &trades_005,
    );

    buba_paint::db::build_data::build_market_data(
        runs_dir.to_str().unwrap(),
        output_path.to_str().unwrap(),
    )
    .unwrap();

    let db = Connection::open(&output_path).unwrap();

    let run_count: i64 = db
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(run_count, 2, "should have 2 runs");

    let tick_count: i64 = db
        .query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tick_count, 8, "should have 8 ticks total");

    let market_count: i64 = db
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        market_count, 3,
        "should have 3 unique markets (dedup mkt-1)"
    );

    let trade_count: i64 = db
        .query_row("SELECT COUNT(*) FROM historical_trades", [], |r| r.get(0))
        .unwrap();
    assert_eq!(trade_count, 3, "should have 3 trades total");

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

    let settled_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM markets WHERE outcome IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        settled_count > 0,
        "at least one market should have computed settlement"
    );

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
