use super::*;
use crate::types::{
    MarketWindow, Signal, SignalDirection, SimulatedTrade, TradeResult, TradeStatus,
};
use tempfile::NamedTempFile;

/// Helper: create a `Database` backed by a fresh temporary file.
fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

fn sample_market_window() -> MarketWindow {
    MarketWindow {
        market_id: "mkt-1".into(),
        question: "Will BTC go up?".into(),
        condition_id: "cond-1".into(),
        slug: "btc-up-down".into(),
        up_token_id: "tok-up".into(),
        down_token_id: "tok-down".into(),
        start_time: 1_700_000_000_000,
        end_time: 1_700_000_300_000,
    }
}

fn sample_trade() -> SimulatedTrade {
    SimulatedTrade {
        id: None,
        timestamp: 1_700_000_000_000,
        market_id: "mkt-1".into(),
        strategy: "latency-arb".into(),
        side: SignalDirection::Up,
        token_id: "tok-up".into(),
        entry_price: 0.52,
        size: 50.0,
        status: TradeStatus::Open,
    }
}

fn sample_signal() -> Signal {
    Signal {
        timestamp: 1_700_000_000_000,
        strategy: "latency-arb".into(),
        direction: SignalDirection::Up,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.52,
        down_ask: 0.50,
        up_bid: 0.48,
        down_bid: 0.46,
        metadata: serde_json::json!({"momentum": 0.0015}),
    }
}

// -- tick_data ------------------------------------------------------------

#[test]
fn insert_tick_and_verify_count() {
    let (db, _tmp) = temp_db();
    db.log_tick(
        1_700_000_000_000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.log_tick(
        1_700_000_000_001,
        "clob_up",
        None,
        Some(0.45),
        Some(0.55),
        Some(100.0),
        Some(200.0),
    )
    .unwrap();

    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn tick_source_check_constraint() {
    let (db, _tmp) = temp_db();
    let result = db.log_tick(1_000, "invalid_source", Some(1.0), None, None, None, None);
    assert!(result.is_err());
}

// -- markets --------------------------------------------------------------

#[test]
fn upsert_market_insert_and_update() {
    let (db, _tmp) = temp_db();
    let window = sample_market_window();

    db.upsert_market(&window).unwrap();

    // Verify it exists.
    let q: String = db
        .conn
        .query_row(
            "SELECT question FROM markets WHERE market_id = ?1",
            params![window.market_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(q, "Will BTC go up?");

    // Upsert with changed question.
    let mut updated = window.clone();
    updated.question = "Updated question".into();
    db.upsert_market(&updated).unwrap();

    let q2: String = db
        .conn
        .query_row(
            "SELECT question FROM markets WHERE market_id = ?1",
            params![updated.market_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(q2, "Updated question");

    // Still only one row.
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

// -- signals --------------------------------------------------------------

#[test]
fn log_signal_stores_metadata_as_json() {
    let (db, _tmp) = temp_db();
    let signal = sample_signal();
    db.log_signal(&signal).unwrap();

    let meta: String = db
        .conn
        .query_row("SELECT metadata FROM signals LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&meta).unwrap();
    assert!((parsed["momentum"].as_f64().unwrap() - 0.0015).abs() < f64::EPSILON);
}

#[test]
fn log_signal_direction_stored_as_text() {
    let (db, _tmp) = temp_db();
    db.log_signal(&sample_signal()).unwrap();

    let dir: String = db
        .conn
        .query_row("SELECT direction FROM signals LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dir, "UP");
}

// -- simulated_trades -----------------------------------------------------

#[test]
fn open_trade_returns_id() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let trade = sample_trade();
    let id = db.open_trade(&trade).unwrap();
    assert!(id > 0);
}

#[test]
fn open_trade_sequential_ids() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let id1 = db.open_trade(&sample_trade()).unwrap();
    let id2 = db.open_trade(&sample_trade()).unwrap();
    assert_eq!(id2, id1 + 1);
}

// -- close_trade ----------------------------------------------------------

#[test]
fn close_trade_updates_status_and_inserts_result() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let trade_id = db.open_trade(&sample_trade()).unwrap();

    let result = TradeResult {
        trade_id,
        exit_price: 0.60,
        settlement_price: 1.0,
        pnl_0pct: 9.23,
        pnl_1pct: 8.73,
        pnl_2pct: 8.23,
        pnl_3pct: 7.73,
    };
    db.close_trade(trade_id, &result).unwrap();

    // Status should be "closed".
    let status: String = db
        .conn
        .query_row(
            "SELECT status FROM simulated_trades WHERE id = ?1",
            params![trade_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "closed");

    // trade_results row should exist.
    let pnl: f64 = db
        .conn
        .query_row(
            "SELECT pnl_0pct FROM trade_results WHERE trade_id = ?1",
            params![trade_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!((pnl - 9.23).abs() < f64::EPSILON);

    // resolved_at should be a recent wall-clock timestamp (within last 5 s).
    let resolved_at: u64 = db
        .conn
        .query_row(
            "SELECT resolved_at FROM trade_results WHERE trade_id = ?1",
            params![trade_id],
            |r| r.get(0),
        )
        .unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(now - resolved_at < 5_000);
}

// -- get_open_trades_for_market -------------------------------------------

#[test]
fn get_open_trades_for_market_filters_correctly() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    // Insert two open trades and one closed trade.
    let id1 = db.open_trade(&sample_trade()).unwrap();
    let _id2 = db.open_trade(&sample_trade()).unwrap();

    // Close the first one.
    let result = TradeResult {
        trade_id: id1,
        exit_price: 0.60,
        settlement_price: 1.0,
        pnl_0pct: 5.0,
        pnl_1pct: 4.5,
        pnl_2pct: 4.0,
        pnl_3pct: 3.5,
    };
    db.close_trade(id1, &result).unwrap();

    let open_trades = db.get_open_trades_for_market("mkt-1").unwrap();
    assert_eq!(open_trades.len(), 1);
    assert_eq!(open_trades[0].status, TradeStatus::Open);
    assert!(open_trades[0].id.is_some());
}

#[test]
fn get_open_trades_for_unknown_market_returns_empty() {
    let (db, _tmp) = temp_db();
    let trades = db.get_open_trades_for_market("nonexistent").unwrap();
    assert!(trades.is_empty());
}

// -- balance_log ----------------------------------------------------------

#[test]
fn log_balance_event_and_get_latest() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();
    let trade_id = db.open_trade(&sample_trade()).unwrap();

    db.log_balance_event(1_000, "init", None, 200.0, 200.0)
        .unwrap();
    db.log_balance_event(2_000, "trade_pnl", Some(trade_id), 50.0, 250.0)
        .unwrap();

    let latest = db.get_latest_balance().unwrap();
    assert_eq!(latest, Some(250.0));
}

#[test]
fn get_latest_balance_empty_db() {
    let (db, _tmp) = temp_db();
    let latest = db.get_latest_balance().unwrap();
    assert_eq!(latest, None);
}

// -- resolve_market -------------------------------------------------------

#[test]
fn resolve_market_changes_status() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    db.resolve_market("mkt-1", "resolved").unwrap();

    let status: String = db
        .conn
        .query_row(
            "SELECT status FROM markets WHERE market_id = ?1",
            params!["mkt-1"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "resolved");
}

#[test]
fn resolve_market_to_closed() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    db.resolve_market("mkt-1", "closed").unwrap();

    let status: String = db
        .conn
        .query_row(
            "SELECT status FROM markets WHERE market_id = ?1",
            params!["mkt-1"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "closed");
}

// -- close ----------------------------------------------------------------

#[test]
fn close_is_callable() {
    let (db, _tmp) = temp_db();
    db.close(); // should not panic
}

// -- constructor ----------------------------------------------------------

#[test]
fn new_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b").join("test.db");
    let db = Database::new(nested.to_str().unwrap()).unwrap();
    db.close();
    assert!(nested.exists());
}

#[test]
fn wal_mode_enabled() {
    let (db, _tmp) = temp_db();
    let mode: String = db
        .conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn synchronous_is_normal() {
    let (db, _tmp) = temp_db();
    let sync: i64 = db
        .conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .unwrap();
    // NORMAL = 1
    assert_eq!(sync, 1);
}

// -- Phase D: additional edge-case tests ----------------------------------

#[test]
fn log_tick_all_valid_sources() {
    let (db, _tmp) = temp_db();
    let sources = ["binance", "clob_up", "clob_down", "chainlink"];
    for (i, source) in sources.iter().enumerate() {
        db.log_tick(
            1_700_000_000_000 + i as u64,
            source,
            Some(42_000.0),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    }
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 4);
}

#[test]
fn log_tick_multiple_per_source_and_count() {
    let (db, _tmp) = temp_db();
    // 3 binance, 2 clob_up, 1 clob_down, 0 chainlink
    for i in 0..3 {
        db.log_tick(1_000 + i, "binance", Some(42_000.0), None, None, None, None)
            .unwrap();
    }
    for i in 0..2 {
        db.log_tick(
            2_000 + i,
            "clob_up",
            None,
            Some(0.45),
            Some(0.55),
            Some(100.0),
            Some(200.0),
        )
        .unwrap();
    }
    db.log_tick(
        3_000,
        "clob_down",
        None,
        Some(0.44),
        Some(0.56),
        Some(150.0),
        Some(250.0),
    )
    .unwrap();

    let count_binance: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM tick_data WHERE source = 'binance'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count_binance, 3);

    let count_clob_up: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM tick_data WHERE source = 'clob_up'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count_clob_up, 2);

    let count_clob_down: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM tick_data WHERE source = 'clob_down'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count_clob_down, 1);

    let count_chainlink: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM tick_data WHERE source = 'chainlink'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count_chainlink, 0);
}

#[test]
fn upsert_market_idempotent_same_data() {
    let (db, _tmp) = temp_db();
    let window = sample_market_window();

    // Insert the same market three times with identical data.
    db.upsert_market(&window).unwrap();
    db.upsert_market(&window).unwrap();
    db.upsert_market(&window).unwrap();

    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "idempotent upsert should keep exactly 1 row");
}

#[test]
fn log_tick_with_all_null_optional_fields() {
    let (db, _tmp) = temp_db();
    // Binance tick: only price set, everything else None.
    db.log_tick(1_000, "binance", Some(42_000.0), None, None, None, None)
        .unwrap();

    // Read back and verify NULLs.
    let (bid, ask): (Option<f64>, Option<f64>) = db
        .conn
        .query_row(
            "SELECT bid, ask FROM tick_data WHERE source = 'binance' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(bid.is_none());
    assert!(ask.is_none());
}

#[test]
fn log_signal_down_direction() {
    let (db, _tmp) = temp_db();
    let mut signal = sample_signal();
    signal.direction = SignalDirection::Down;
    db.log_signal(&signal).unwrap();

    let dir: String = db
        .conn
        .query_row("SELECT direction FROM signals LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dir, "DOWN");
}

#[test]
fn log_balance_event_without_trade_id() {
    let (db, _tmp) = temp_db();
    db.log_balance_event(1_000, "init", None, 0.0, 200.0)
        .unwrap();

    let trade_id: Option<i64> = db
        .conn
        .query_row("SELECT trade_id FROM balance_log LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(trade_id.is_none());
}

#[test]
fn open_trade_down_side() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let mut trade = sample_trade();
    trade.side = SignalDirection::Down;
    trade.token_id = "tok-down".into();
    let id = db.open_trade(&trade).unwrap();
    assert!(id > 0);

    let side: String = db
        .conn
        .query_row(
            "SELECT side FROM simulated_trades WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(side, "DOWN");
}

#[test]
fn resolve_market_nonexistent_is_noop() {
    let (db, _tmp) = temp_db();
    // Resolving a market that doesn't exist shouldn't error.
    db.resolve_market("nonexistent-mkt", "resolved").unwrap();
}

#[test]
fn get_open_trades_returns_correct_fields() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let mut trade = sample_trade();
    trade.entry_price = 0.45;
    trade.size = 25.0;
    let id = db.open_trade(&trade).unwrap();

    let open = db.get_open_trades_for_market("mkt-1").unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, Some(id));
    assert_eq!(open[0].market_id, "mkt-1");
    assert_eq!(open[0].strategy, "latency-arb");
    assert_eq!(open[0].side, SignalDirection::Up);
    assert_eq!(open[0].token_id, "tok-up");
    assert!((open[0].entry_price - 0.45).abs() < f64::EPSILON);
    assert!((open[0].size - 25.0).abs() < f64::EPSILON);
    assert_eq!(open[0].status, TradeStatus::Open);
}
