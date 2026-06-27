use super::*;
use crate::types::{
    FeedEvent, MarketWindow, ReplayFidelity, Signal, SignalDirection, SimulatedTrade,
    StrategyRejectionSummaryRecord, TradeResult, TradeStatus,
};
use tempfile::NamedTempFile;

/// Helper: create a `Database` backed by a fresh temporary file.
fn temp_db() -> (Database, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::new(tmp.path().to_str().unwrap()).unwrap();
    (db, tmp)
}

/// Verifies that runtime opens do not run schema migrations.
#[test]
fn open_runtime_does_not_create_schema() {
    let tmp = NamedTempFile::new().unwrap();
    let db_path = tmp.path().to_str().unwrap();
    let db = Database::open_runtime(db_path).unwrap();
    let table_count: u64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'run_metadata'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    db.close();

    assert_eq!(table_count, 0);
}

/// Sample market window.
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
        outcome: None,
        resolution_source: Some("chainlink".into()),
        fee_profile: Some("crypto".into()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
        fees_enabled: Some(true),
        fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".into()),
        token_fee_rates_json: Some("{\"tok-up\":{\"base_fee\":1000}}".into()),
        accepting_orders: Some(true),
        accepting_orders_timestamp: Some("2024-01-01T00:00:00Z".into()),
        clear_book_on_start: Some(false),
    }
}

/// Sample trade.
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
    }
}

/// Sample signal.
fn sample_signal() -> Signal {
    Signal {
        timestamp: 1_700_000_000_000,
        strategy: "latency-arb".into(),
        strategy_version: "v2".into(),
        feature_mode: "legacy_core".into(),
        direction: SignalDirection::Up,
        confidence: 0.72,
        binance_price: 42_000.0,
        chainlink_price: 41_999.0,
        up_ask: 0.52,
        down_ask: 0.50,
        up_bid: 0.48,
        down_bid: 0.46,
        expected_edge: None,
        metadata: serde_json::json!({"momentum": 0.0015}),
        telemetry: None,
    }
}

/// Sample feed event.
fn sample_feed_event() -> FeedEvent {
    FeedEvent {
        id: None,
        received_at_ms: 1_700_000_000_000,
        event_at_ms: 1_700_000_000_000,
        received_at_us: Some(1_700_000_000_000_000),
        event_at_us: Some(1_700_000_000_000_000),
        source: "binance".into(),
        event_type: "aggTrade".into(),
        source_topic: Some("btcusdt@aggTrade".into()),
        source_symbol: Some("BTCUSDT".into()),
        connection_id: Some("binance-1".into()),
        sequence_key: Some("123".into()),
        market_id: Some("mkt-1".into()),
        asset_id: None,
        price: Some(42_000.0),
        trade_size: Some(0.25),
        signed_quantity: Some(-0.25),
        best_bid: None,
        best_ask: None,
        bid_size: None,
        ask_size: None,
        depth_bid_notional: None,
        depth_ask_notional: None,
        depth_imbalance: None,
        microprice: None,
        payload_json: Some("{\"raw\":true}".into()),
        details_json: Some("{\"debug\":true}".into()),
        fidelity: ReplayFidelity::RawEvent,
    }
}

/// Sample compact CLOB top-of-book feed event.
fn sample_clob_feed_event() -> FeedEvent {
    FeedEvent {
        source: "clob_up".into(),
        event_type: "price_change".into(),
        source_topic: Some("market".into()),
        source_symbol: None,
        connection_id: Some("clob-1".into()),
        sequence_key: Some("seq-1".into()),
        asset_id: Some("tok-up".into()),
        price: None,
        trade_size: None,
        signed_quantity: None,
        best_bid: Some(0.48),
        best_ask: Some(0.52),
        bid_size: Some(10.0),
        ask_size: Some(20.0),
        microprice: Some(0.49333333333333335),
        ..sample_feed_event()
    }
}

/// Sample rejection-summary row.
fn sample_rejection_summary() -> StrategyRejectionSummaryRecord {
    StrategyRejectionSummaryRecord {
        timestamp_ms: 1_700_000_000_000,
        market_id: "mkt-1".into(),
        strategy: "latency-arb".into(),
        reason: "direction_not_selected".into(),
        count: 7,
        details_json: serde_json::json!({
            "last": {"upAsk": 0.54},
            "mean": {"upAsk": 0.51}
        })
        .to_string(),
    }
}

/// Verifies that insert tick and verify count.
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

/// Verifies that execution outcomes require a pre-existing telemetry row.
#[test]
fn update_signal_execution_outcome_requires_existing_signal_metric_row() {
    let (db, _tmp) = temp_db();
    let error = db
        .update_signal_execution_outcome(999, 1_000, Some(1_000_000), 250, "missed", Some("x"))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("expected exactly one signal_metrics row")
    );
}

/// Verifies that tick source check constraint.
#[test]
fn tick_source_check_constraint() {
    let (db, _tmp) = temp_db();
    let result = db.log_tick(1_000, "invalid_source", Some(1.0), None, None, None, None);
    assert!(result.is_err());
}

/// Verifies that upsert market insert and update.
#[test]
fn upsert_market_insert_and_update() {
    let (db, _tmp) = temp_db();
    let window = sample_market_window();

    db.upsert_market(&window).unwrap();

    let q: String = db
        .conn
        .query_row(
            "SELECT question FROM markets WHERE market_id = ?1",
            params![window.market_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(q, "Will BTC go up?");

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

    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

/// Verifies that log signal stores metadata as json.
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

/// Verifies that rejection-summary rows are persisted with their details payload.
#[test]
fn log_strategy_rejection_summary_persists_row() {
    let (db, _tmp) = temp_db();
    let summary = sample_rejection_summary();
    db.log_strategy_rejection_summary(&summary).unwrap();

    let (count, details_json): (i64, String) = db
        .conn
        .query_row(
            "SELECT count, details_json FROM strategy_rejection_summaries LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 7);
    let details: serde_json::Value = serde_json::from_str(&details_json).unwrap();
    assert_eq!(details["mean"]["upAsk"], 0.51);
}

/// Verifies that unresolved-trade exposures include market timing for reserve recovery.
#[test]
fn unresolved_trade_exposures_include_market_end_times() {
    let (db, _tmp) = temp_db();
    let window = sample_market_window();
    db.upsert_market(&window).unwrap();

    let trade_id = db.open_trade(&sample_trade()).unwrap();

    let exposures = db.unresolved_trade_exposures().unwrap();
    assert_eq!(exposures.len(), 1);
    assert_eq!(exposures[0].trade_id, trade_id);
    assert_eq!(exposures[0].market_id, window.market_id);
    assert_eq!(exposures[0].strategy, "latency-arb");
    assert_eq!(exposures[0].market_end_time, window.end_time);
}

/// Verifies that active and pending open-trade counts split around market close.
#[test]
fn count_active_and_pending_open_trades_split_by_market_end() {
    let (db, _tmp) = temp_db();

    let active_window = sample_market_window();
    db.upsert_market(&active_window).unwrap();
    db.open_trade(&sample_trade()).unwrap();

    let mut pending_window = sample_market_window();
    pending_window.market_id = "mkt-2".into();
    pending_window.up_token_id = "tok-up-2".into();
    pending_window.down_token_id = "tok-down-2".into();
    pending_window.end_time = active_window.end_time - 1_000;
    db.upsert_market(&pending_window).unwrap();
    let mut pending_trade = sample_trade();
    pending_trade.market_id = pending_window.market_id.clone();
    pending_trade.token_id = pending_window.up_token_id.clone();
    db.open_trade(&pending_trade).unwrap();

    let now_ms = active_window.end_time - 500;
    assert_eq!(db.count_active_open_trades(now_ms).unwrap(), 1);
    assert_eq!(db.count_pending_settlement_open_trades(now_ms).unwrap(), 1);
}

/// Verifies that earliest Binance price lookup returns the first tick inside the window.
#[test]
fn earliest_binance_price_in_window_returns_first_tick() {
    let (db, _tmp) = temp_db();
    db.log_tick(1_000, "binance", Some(42_100.0), None, None, None, None)
        .unwrap();
    db.log_tick(2_000, "binance", Some(42_200.0), None, None, None, None)
        .unwrap();
    db.log_tick(3_000, "binance", Some(42_300.0), None, None, None, None)
        .unwrap();

    let price = db.earliest_binance_price_in_window(1_500, 3_500).unwrap();
    assert_eq!(price, Some(42_200.0));
}

/// Verifies that earliest Binance price lookup returns none when no tick exists.
#[test]
fn earliest_binance_price_in_window_returns_none_for_empty_window() {
    let (db, _tmp) = temp_db();
    let price = db.earliest_binance_price_in_window(1_500, 3_500).unwrap();
    assert!(price.is_none());
}

/// Verifies that log signal direction stored as text.
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

/// Verifies that feed-event logging stores compact typed fields.
#[test]
fn log_feed_event_stores_compact_columns() {
    let (db, _tmp) = temp_db();
    db.log_feed_event(&sample_feed_event()).unwrap();
    let (trade_size, signed_quantity): (f64, f64) = db
        .conn
        .query_row(
            "SELECT trade_size, signed_quantity FROM feed_events LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!((trade_size - 0.25).abs() < f64::EPSILON);
    assert!((signed_quantity + 0.25).abs() < f64::EPSILON);
}

/// Verifies that replay-grade batch logging routes CLOB rows to compact storage.
#[test]
fn log_feed_events_batch_routes_clob_rows_to_compact_storage() {
    let (db, _tmp) = temp_db();
    db.log_feed_events_batch_with_compact_clob(&[sample_clob_feed_event()], true)
        .unwrap();

    let feed_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
        .unwrap();
    let compact_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(feed_count, 0);
    assert_eq!(compact_count, 1);
}

/// Verifies that block logging routes replay-grade CLOB rows to compressed blocks.
#[test]
fn log_feed_events_and_clob_block_routes_clob_rows_to_block_storage() {
    let (db, _tmp) = temp_db();
    db.log_feed_events_and_clob_block(&[], &[sample_clob_feed_event()], 1)
        .unwrap();

    let feed_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
        .unwrap();
    let compact_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    let block_rows: u64 = db
        .conn()
        .query_row(
            "SELECT COALESCE(SUM(row_count), 0) FROM clob_replay_blocks",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(feed_count, 0);
    assert_eq!(compact_count, 0);
    assert_eq!(block_rows, 1);
}

/// Verifies that full-debug style batch logging keeps CLOB rows generic.
#[test]
fn log_feed_events_batch_without_compact_routing_keeps_clob_rows_generic() {
    let (db, _tmp) = temp_db();
    db.log_feed_events_batch_with_compact_clob(&[sample_clob_feed_event()], false)
        .unwrap();

    let feed_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
        .unwrap();
    let compact_count: u64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM clob_replay_events", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(feed_count, 1);
    assert_eq!(compact_count, 0);
}

/// Verifies that storage footprint reports grouped feed-event counts.
#[test]
fn storage_footprint_reports_grouped_feed_events() {
    let (db, _tmp) = temp_db();
    db.log_feed_event(&sample_feed_event()).unwrap();
    let footprint = db.storage_footprint().unwrap();
    assert_eq!(footprint.feed_event_count, 1);
    assert_eq!(footprint.grouped_feed_events.len(), 1);
    assert_eq!(footprint.grouped_feed_events[0].source, "binance");
    assert_eq!(footprint.grouped_feed_events[0].event_type, "aggTrade");
}

/// Verifies that run metadata upserts stable values.
#[test]
fn run_metadata_upserts_values() {
    let (db, _tmp) = temp_db();
    db.set_run_metadata("replay_quality_class", "descriptive_only", 1_000)
        .unwrap();
    db.set_run_metadata("replay_quality_class", "sweep_grade", 2_000)
        .unwrap();
    let value = db.get_run_metadata("replay_quality_class").unwrap();
    assert_eq!(value.as_deref(), Some("sweep_grade"));
}

/// Verifies that open trade returns id.
#[test]
fn open_trade_returns_id() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let trade = sample_trade();
    let id = db.open_trade(&trade).unwrap();
    assert!(id > 0);
}

/// Verifies that open trade sequential ids.
#[test]
fn open_trade_sequential_ids() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let id1 = db.open_trade(&sample_trade()).unwrap();
    let id2 = db.open_trade(&sample_trade()).unwrap();
    assert_eq!(id2, id1 + 1);
}

/// Verifies that close trade updates status and inserts result.
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
        fee_amount: 0.12,
        pnl_net: 9.11,
        settlement_status: "confirmed".to_string(),
        provisional_pnl: None,
    };
    db.close_trade(trade_id, &result).unwrap();

    let status: String = db
        .conn
        .query_row(
            "SELECT status FROM simulated_trades WHERE id = ?1",
            params![trade_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "closed");

    let pnl: f64 = db
        .conn
        .query_row(
            "SELECT pnl_0pct FROM trade_results WHERE trade_id = ?1",
            params![trade_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!((pnl - 9.23).abs() < f64::EPSILON);

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

/// Verifies that get open trades for market filters correctly.
#[test]
fn get_open_trades_for_market_filters_correctly() {
    let (db, _tmp) = temp_db();
    db.upsert_market(&sample_market_window()).unwrap();

    let id1 = db.open_trade(&sample_trade()).unwrap();
    let _id2 = db.open_trade(&sample_trade()).unwrap();

    let result = TradeResult {
        trade_id: id1,
        exit_price: 0.60,
        settlement_price: 1.0,
        pnl_0pct: 5.0,
        pnl_1pct: 4.5,
        pnl_2pct: 4.0,
        pnl_3pct: 3.5,
        fee_amount: 0.05,
        pnl_net: 3.95,
        settlement_status: "provisional".to_string(),
        provisional_pnl: Some(3.95),
    };
    db.close_trade(id1, &result).unwrap();

    let open_trades = db.get_open_trades_for_market("mkt-1").unwrap();
    assert_eq!(open_trades.len(), 1);
    assert_eq!(open_trades[0].status, TradeStatus::Open);
    assert!(open_trades[0].id.is_some());
}

/// Verifies that get open trades for unknown market returns empty.
#[test]
fn get_open_trades_for_unknown_market_returns_empty() {
    let (db, _tmp) = temp_db();
    let trades = db.get_open_trades_for_market("nonexistent").unwrap();
    assert!(trades.is_empty());
}

/// Verifies that log balance event and get latest.
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

/// Verifies that get latest balance empty db.
#[test]
fn get_latest_balance_empty_db() {
    let (db, _tmp) = temp_db();
    let latest = db.get_latest_balance().unwrap();
    assert_eq!(latest, None);
}

/// Verifies that resolve market changes status.
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

/// Verifies that resolve market to closed.
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

/// Verifies that close is callable.
#[test]
fn close_is_callable() {
    let (db, _tmp) = temp_db();
    db.close();
}

/// Verifies that new creates parent dirs.
#[test]
fn new_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b").join("test.db");
    let db = Database::new(nested.to_str().unwrap()).unwrap();
    db.close();
    assert!(nested.exists());
}

/// Verifies that wal mode enabled.
#[test]
fn wal_mode_enabled() {
    let (db, _tmp) = temp_db();
    let mode: String = db
        .conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

/// Verifies that synchronous is normal.
#[test]
fn synchronous_is_normal() {
    let (db, _tmp) = temp_db();
    let sync: i64 = db
        .conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .unwrap();

    assert_eq!(sync, 1);
}

/// Verifies that log tick all valid sources.
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

/// Verifies that log tick multiple per source and count.
#[test]
fn log_tick_multiple_per_source_and_count() {
    let (db, _tmp) = temp_db();

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

/// Verifies that upsert market idempotent same data.
#[test]
fn upsert_market_idempotent_same_data() {
    let (db, _tmp) = temp_db();
    let window = sample_market_window();

    db.upsert_market(&window).unwrap();
    db.upsert_market(&window).unwrap();
    db.upsert_market(&window).unwrap();

    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "idempotent upsert should keep exactly 1 row");
}

/// Verifies that log tick with all null optional fields.
#[test]
fn log_tick_with_all_null_optional_fields() {
    let (db, _tmp) = temp_db();

    db.log_tick(1_000, "binance", Some(42_000.0), None, None, None, None)
        .unwrap();

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

/// Verifies that log signal down direction.
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

/// Verifies that log balance event without trade id.
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

/// Verifies that open trade down side.
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

/// Verifies that resolve market nonexistent is noop.
#[test]
fn resolve_market_nonexistent_is_noop() {
    let (db, _tmp) = temp_db();

    db.resolve_market("nonexistent-mkt", "resolved").unwrap();
}

/// Verifies that get open trades returns correct fields.
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

/// Verifies that unresolved open-trade market recovery returns only ended unresolved markets.
#[test]
fn unresolved_open_trade_markets_returns_only_ended_unresolved_rows() {
    let (db, _tmp) = temp_db();

    let ended = sample_market_window();
    db.upsert_market(&ended).unwrap();
    db.open_trade(&sample_trade()).unwrap();

    let mut future = sample_market_window();
    future.market_id = "mkt-future".into();
    future.condition_id = "cond-future".into();
    future.slug = "btc-up-down-future".into();
    future.up_token_id = "tok-up-future".into();
    future.down_token_id = "tok-down-future".into();
    future.start_time = 1_700_000_600_000;
    future.end_time = 1_700_000_900_000;
    db.upsert_market(&future).unwrap();
    let mut future_trade = sample_trade();
    future_trade.market_id = future.market_id.clone();
    future_trade.token_id = future.up_token_id.clone();
    db.open_trade(&future_trade).unwrap();

    let mut settled = sample_market_window();
    settled.market_id = "mkt-settled".into();
    settled.condition_id = "cond-settled".into();
    settled.slug = "btc-up-down-settled".into();
    settled.up_token_id = "tok-up-settled".into();
    settled.down_token_id = "tok-down-settled".into();
    db.upsert_market(&settled).unwrap();
    let mut settled_trade = sample_trade();
    settled_trade.market_id = settled.market_id.clone();
    settled_trade.token_id = settled.up_token_id.clone();
    let settled_trade_id = db.open_trade(&settled_trade).unwrap();
    db.close_trade(
        settled_trade_id,
        &TradeResult {
            trade_id: settled_trade_id,
            exit_price: 1.0,
            settlement_price: 1.0,
            pnl_0pct: 5.0,
            pnl_1pct: 4.0,
            pnl_2pct: 3.0,
            pnl_3pct: 2.0,
            fee_amount: 0.1,
            pnl_net: 4.9,
            settlement_status: "confirmed".to_string(),
            provisional_pnl: None,
        },
    )
    .unwrap();

    let rows = db.unresolved_open_trade_markets(1_700_000_500_000).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].window.market_id, "mkt-1");
    assert_eq!(rows[0].open_trade_count, 1);
}

/// Verifies the live-order idempotency primitives: index presence, intent lookup, and the
/// durable venue-attempt compare-and-set.
#[test]
fn live_order_idempotency_primitives() {
    let (db, _tmp) = temp_db();
    assert!(db.live_order_idempotency_ready().unwrap());
    let session_id = db
        .insert_live_session(&crate::types::LiveSession {
            id: None,
            started_at_ms: 1_000,
            ended_at_ms: None,
            status: "armed".into(),
            execution_mode: "live_trading".into(),
            wallet_address: Some("0xwallet".into()),
            proxy_wallet: Some("0xproxy".into()),
            enabled_strategies_json: "[]".into(),
            config_fingerprint: "fp".into(),
            cash_cap_usd: 100.0,
            details_json: Some("{}".into()),
        })
        .unwrap();
    let intent_id = db
        .log_live_order_intent(&crate::types::LiveOrderIntent {
            id: None,
            session_id,
            signal_id: None,
            market_id: "mkt-1".into(),
            strategy: "latency-arb".into(),
            side: "BUY".into(),
            order_type: "FOK".into(),
            status: "submitted".into(),
            created_at_ms: 1_050,
            requested_price: Some(0.51),
            requested_size: Some(5.0),
            limit_price: Some(0.51),
            fee_schedule_json: None,
            token_fee_rates_json: None,
            execution_group_id: None,
            details_json: None,
        })
        .unwrap();
    assert!(db.find_live_order_by_intent(intent_id).unwrap().is_none());
    assert!(db.mark_intent_venue_attempted(intent_id, 1_100).unwrap());
    assert!(!db.mark_intent_venue_attempted(intent_id, 1_200).unwrap());
}

/// Verifies that live session/account/order tables accept compact live telemetry.
#[test]
fn live_telemetry_tables_store_rows() {
    let (db, _tmp) = temp_db();

    let session_id = db
        .insert_live_session(&crate::types::LiveSession {
            id: None,
            started_at_ms: 1_000,
            ended_at_ms: None,
            status: "readonly_ready".into(),
            execution_mode: "live_readonly".into(),
            wallet_address: Some("0xwallet".into()),
            proxy_wallet: Some("0xproxy".into()),
            enabled_strategies_json: "[\"latency-arb\"]".into(),
            config_fingerprint: "fp-1".into(),
            cash_cap_usd: 100.0,
            details_json: Some("{}".into()),
        })
        .unwrap();

    let intent_id = db
        .log_live_order_intent(&crate::types::LiveOrderIntent {
            id: None,
            session_id,
            signal_id: None,
            market_id: "mkt-1".into(),
            strategy: "latency-arb".into(),
            side: "BUY".into(),
            order_type: "FOK".into(),
            status: "approved".into(),
            created_at_ms: 1_050,
            requested_price: Some(0.51),
            requested_size: Some(5.0),
            limit_price: Some(0.51),
            fee_schedule_json: Some("{\"exponent\":1,\"rate\":0.072}".into()),
            token_fee_rates_json: Some("{\"tok-up\":{\"base_fee\":1000}}".into()),
            execution_group_id: Some("grp-1".into()),
            details_json: Some("{}".into()),
        })
        .unwrap();

    let order_id = db
        .log_live_order(&crate::types::LiveOrder {
            id: None,
            session_id,
            intent_id,
            venue_order_id: Some("venue-1".into()),
            client_order_id: Some("client-1".into()),
            market_id: "mkt-1".into(),
            token_id: Some("tok-up".into()),
            side: "BUY".into(),
            order_type: "FOK".into(),
            status: "open".into(),
            status_reason: None,
            created_at_ms: 1_060,
            acknowledged_at_ms: Some(1_061),
            updated_at_ms: 1_061,
            requested_price: Some(0.51),
            limit_price: Some(0.51),
            requested_size: Some(5.0),
            accepted_size: Some(5.0),
            details_json: Some("{}".into()),
        })
        .unwrap();

    db.log_live_fill(&crate::types::LiveFill {
        id: None,
        session_id,
        intent_id: Some(intent_id),
        live_order_id: Some(order_id),
        venue_trade_id: Some("trade-1".into()),
        filled_at_ms: 1_070,
        price: 0.51,
        size: 5.0,
        fee_amount: Some(0.09),
        fee_rate: Some(0.072),
        liquidity_side: Some("taker".into()),
        tx_hash: Some("0xtx".into()),
        status: "confirmed".into(),
        details_json: Some("{}".into()),
    })
    .unwrap();

    db.log_live_account_snapshot(&crate::types::LiveAccountSnapshot {
        id: None,
        session_id,
        timestamp_ms: 1_080,
        cash_available: 96.0,
        cash_reserved_for_orders: 0.0,
        inventory_mark_value: 2.0,
        redeemable_value: 0.0,
        pending_redeem_value: 0.0,
        total_equity: 98.0,
        allowance_available: Some(96.0),
        details_json: Some("{}".into()),
    })
    .unwrap();

    let session_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM live_sessions", [], |row| row.get(0))
        .unwrap();
    let order_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM live_orders", [], |row| row.get(0))
        .unwrap();
    let fill_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM live_fills", [], |row| row.get(0))
        .unwrap();
    let snapshot_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM live_account_snapshots", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(session_count, 1);
    assert_eq!(order_count, 1);
    assert_eq!(fill_count, 1);
    assert_eq!(snapshot_count, 1);
}
