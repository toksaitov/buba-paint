use super::*;
use crate::db::schema::run_migrations;
use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

/// Build one migrated in-memory database for live-fidelity tests.
fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Insert one public feed row for replay-grade fixtures.
#[allow(clippy::too_many_arguments)]
fn insert_feed_event(
    conn: &Connection,
    received_at_ms: u64,
    source: &str,
    event_type: &str,
    market_id: Option<&str>,
    asset_id: Option<&str>,
    price: Option<f64>,
    trade_size: Option<f64>,
    signed_quantity: Option<f64>,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
    depth_bid_notional: Option<f64>,
    depth_ask_notional: Option<f64>,
    depth_imbalance: Option<f64>,
) {
    conn.execute(
        "INSERT INTO feed_events (
            received_at_ms,
            event_at_ms,
            received_at_us,
            event_at_us,
            source,
            event_type,
            market_id,
            asset_id,
            price,
            trade_size,
            signed_quantity,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            depth_bid_notional,
            depth_ask_notional,
            depth_imbalance,
            fidelity
        ) VALUES (?1, ?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'raw_event')",
        params![
            received_at_ms,
            received_at_ms * 1_000,
            source,
            event_type,
            market_id,
            asset_id,
            price,
            trade_size,
            signed_quantity,
            best_bid,
            best_ask,
            bid_size,
            ask_size,
            depth_bid_notional,
            depth_ask_notional,
            depth_imbalance
        ],
    )
    .unwrap();
}

/// Insert the public feed classes required by replay-grade validation.
fn insert_replay_grade_feed(conn: &Connection) {
    insert_feed_event(
        conn,
        1_000,
        "binance",
        "aggTrade",
        None,
        None,
        Some(42_000.0),
        Some(0.5),
        Some(-0.5),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1_010,
        "binance",
        "bookTicker",
        None,
        None,
        None,
        None,
        None,
        Some(41_999.0),
        Some(42_001.0),
        Some(1.0),
        Some(1.2),
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1_020,
        "binance",
        "depth",
        None,
        None,
        None,
        None,
        None,
        Some(41_999.0),
        Some(42_001.0),
        Some(1.0),
        Some(1.2),
        Some(10_000.0),
        Some(11_000.0),
        Some(-0.047),
    );
    insert_feed_event(
        conn,
        1_030,
        "chainlink",
        "chainlink_price",
        None,
        None,
        Some(42_000.5),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1_040,
        "clob_up",
        "best_bid_ask",
        Some("mkt-1"),
        Some("tok-up"),
        None,
        None,
        None,
        Some(0.49),
        Some(0.50),
        Some(7.0),
        Some(10.0),
        None,
        None,
        None,
    );
    insert_feed_event(
        conn,
        1_045,
        "clob_down",
        "best_bid_ask",
        Some("mkt-1"),
        Some("tok-down"),
        None,
        None,
        None,
        Some(0.49),
        Some(0.50),
        Some(7.0),
        Some(10.0),
        None,
        None,
        None,
    );
}

/// Insert one complete live-trading lifecycle fixture.
fn insert_complete_live_fixture(conn: &Connection) {
    insert_replay_grade_feed(conn);
    conn.execute(
        "INSERT INTO signals (
                timestamp, strategy, direction, binance_price, chainlink_price, up_ask,
                down_ask, up_bid, down_bid, metadata, market_id, execution_fidelity,
                strategy_version, feature_mode
             ) VALUES (
                1500, 'latency-arb', 'UP', 42000.0, 42000.5, 0.50,
                0.50, 0.49, 0.49, '{}', 'mkt-1', 'raw_event',
                'v1', 'raw_event_full'
             )",
        [],
    )
    .unwrap();
    let signal_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO signal_metrics (
            signal_id, generated_at_ms, generated_at_us, order_submitted_at_ms,
            order_submitted_at_us, expected_arrival_at_ms, expected_arrival_at_us,
            order_processed_at_ms, order_processed_at_us, effective_arrival_delay_ms,
            binance_age_ms, chainlink_age_ms, clob_age_ms, quote_age_ms,
            book_staleness_ms, expected_fee, expected_slippage, expected_edge,
            available_feature_count, decision_status, rejection_reason, features_json
         ) VALUES (
            ?1, 1500, 1500000, 1600, 1600000, 1610, 1610000,
            1610, 1610000, 10, 1, 2, 3, 4, 5, 0.01, 0.01, 0.05,
            8, 'submitted', NULL, '{\"featureMode\":\"raw_event_full\",\"eventToDecisionLagMs\":2}'
         )",
        [signal_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO live_sessions (
            id, started_at_ms, ended_at_ms, status, execution_mode, wallet_address,
            proxy_wallet, enabled_strategies_json, config_fingerprint, cash_cap_usd, details_json
         ) VALUES (
            1, 1000, 3000, 'disarmed', 'live_trading', '0xwallet',
            '0xproxy', '[\"latency-arb\"]', 'fp-1', 100.0, '{}'
         )",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO live_control_state (
            session_id, state, updated_at_ms, actor, reason, details_json
         ) VALUES (1, 'armed', 1400, 'operator', 'test arm', '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO control_audit (timestamp_ms, actor, action, target, details_json)
         VALUES (1400, 'operator', 'live_control_state_changed', 'live_control_state:1', '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO live_account_snapshots (
            session_id, timestamp_ms, cash_available, cash_reserved_for_orders,
            inventory_mark_value, redeemable_value, pending_redeem_value, total_equity,
            allowance_available, details_json
         ) VALUES
            (1, 1550, 100.0, 0.0, 0.0, 0.0, 0.0, 100.0, 100.0, '{}'),
            (1, 1700, 95.0, 0.0, 5.0, 0.0, 0.0, 100.0, 95.0, '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO live_order_intents (
            id, session_id, signal_id, market_id, strategy, side, order_type, status,
            created_at_ms, requested_price, requested_size, limit_price, fee_schedule_json,
            token_fee_rates_json, execution_group_id, details_json
         ) VALUES (
            1, 1, ?1, 'mkt-1', 'latency-arb', 'UP', 'FOK', 'submitted',
            1600, 0.50, 5.0, 0.51, '{\"rate\":0.072}',
            '{\"tok-up\":{\"rate\":0.072}}', 'grp-1', '{\"amount_usd\":5.0}'
         )",
        [signal_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO live_orders (
            id, session_id, intent_id, venue_order_id, client_order_id, market_id, token_id,
            side, order_type, status, status_reason, created_at_ms, acknowledged_at_ms,
            updated_at_ms, requested_price, limit_price, requested_size, accepted_size,
            details_json
         ) VALUES (
            1, 1, 1, 'venue-1', 'client-1', 'mkt-1', 'tok-up',
            'BUY', 'FOK', 'matched', NULL, 1600, 1605,
            1610, 0.50, 0.51, 5.0, 5.0,
            '{\"tick_size\":0.01,\"min_order_size\":5,\"fee_details\":{\"rate\":0.072}}'
         )",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO live_fills (
            session_id, intent_id, live_order_id, venue_trade_id, filled_at_ms, price,
            size, fee_amount, fee_rate, liquidity_side, tx_hash, status, details_json
         ) VALUES (
            1, 1, 1, 'trade-1', 1620, 0.50, 5.0, 0.01, 0.072,
            'taker', '0xtx', 'confirmed_from_activity',
            '{\"matched_amount\":\"5\",\"taker_order_id\":\"venue-1\"}'
         )",
        [],
    )
    .unwrap();
}

/// Verifies that a complete live fixture becomes research-grade.
#[test]
fn complete_live_fixture_is_research_grade() {
    let conn = setup_db();
    insert_complete_live_fixture(&conn);

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::ResearchGradeLive);
    assert!(report.missing_required().is_empty());
}

/// Verifies that intervals without funded evidence are explicitly classified.
#[test]
fn interval_without_live_orders_is_no_live_trading() {
    let conn = setup_db();
    insert_replay_grade_feed(&conn);

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::NoLiveTrading);
}

/// Verifies that missing confirmed trade recovery downgrades a live run.
#[test]
fn missing_confirmed_fill_is_descriptive_only() {
    let conn = setup_db();
    insert_complete_live_fixture(&conn);
    conn.execute("DELETE FROM live_fills", []).unwrap();

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::DescriptiveOnlyLive);
    assert!(
        report
            .missing_required_keys()
            .contains(&"confirmed_fill_lifecycle".to_string())
    );
}

/// Verifies that critical reconciliation blocks research-grade classification.
#[test]
fn critical_reconciliation_is_descriptive_only() {
    let conn = setup_db();
    insert_complete_live_fixture(&conn);
    conn.execute(
        "INSERT INTO live_reconciliation_events (
            session_id, timestamp_ms, severity, event_type, local_value, remote_value, details_json
         ) VALUES (1, 1650, 'critical', 'unknown_submission', NULL, NULL, '{}')",
        [],
    )
    .unwrap();

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::DescriptiveOnlyLive);
    assert!(
        report
            .missing_required_keys()
            .contains(&"no_critical_reconciliation".to_string())
    );
}

/// Verifies that missing fee metadata blocks order explainability.
#[test]
fn missing_fee_metadata_is_descriptive_only() {
    let conn = setup_db();
    insert_complete_live_fixture(&conn);
    conn.execute("UPDATE live_orders SET details_json = '{}'", [])
        .unwrap();

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::DescriptiveOnlyLive);
    assert!(
        report
            .missing_required_keys()
            .contains(&"venue_order_fields".to_string())
    );
}

/// Verifies that missing raw signal features block live research-grade use.
#[test]
fn missing_signal_feature_snapshot_is_descriptive_only() {
    let conn = setup_db();
    insert_complete_live_fixture(&conn);
    conn.execute(
        "UPDATE signal_metrics
         SET features_json = '{\"featureMode\":\"legacy_core\"}'",
        [],
    )
    .unwrap();

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::DescriptiveOnlyLive);
    assert!(
        report
            .missing_required_keys()
            .contains(&"signal_feature_snapshots".to_string())
    );
}

/// Verifies that missing marketable book evidence blocks explainability.
#[test]
fn missing_order_book_explainability_is_descriptive_only() {
    let conn = setup_db();
    insert_complete_live_fixture(&conn);
    conn.execute("UPDATE live_orders SET token_id = 'tok-missing'", [])
        .unwrap();

    let report = analyze_connection(&conn, 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::DescriptiveOnlyLive);
    assert!(
        report
            .missing_required_keys()
            .contains(&"order_book_explainability".to_string())
    );
}

/// Verifies that live-fidelity path validation can write a complete fixture.
#[test]
fn validate_live_sweep_input_accepts_complete_fixture_path() {
    let db_file = NamedTempFile::new().unwrap();
    let conn = Connection::open(db_file.path()).unwrap();
    run_migrations(&conn).unwrap();
    insert_complete_live_fixture(&conn);
    drop(conn);

    let report = validate_live_sweep_input(db_file.path().to_str().unwrap(), 0, 4_000).unwrap();

    assert_eq!(report.class, LiveFidelityClass::ResearchGradeLive);
}
