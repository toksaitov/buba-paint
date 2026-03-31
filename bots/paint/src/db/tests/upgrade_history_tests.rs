use crate::db::schema::run_migrations;

use super::*;

#[test]
/// Verify that legacy tick rows are synthesized into `feed_events` exactly once.
fn synthesize_feed_events_backfills_legacy_rows_once() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    conn.execute(
        "INSERT INTO markets (
            market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'resolved')",
        params![
            "mkt-1",
            "Will BTC go up?",
            "cond-1",
            "btc-updown-5m",
            "tok-up",
            "tok-down",
            1_000_i64,
            10_000_i64
        ],
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size) VALUES
            (2_000, 'binance', 68000.0, NULL, NULL, NULL, NULL),
            (2_100, 'chainlink', 68010.0, NULL, NULL, NULL, NULL),
            (2_200, 'clob_up', 0.55, 0.54, 0.55, 50.0, 40.0),
            (2_300, 'clob_down', 0.45, 0.44, 0.45, 35.0, 30.0);",
    )
    .unwrap();

    synthesize_feed_events(&conn).unwrap();
    synthesize_feed_events(&conn).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 4);

    let up_snapshot: (String, String, String, String) = conn
        .query_row(
            "SELECT event_type, market_id, asset_id, fidelity
             FROM feed_events
             WHERE source = 'clob_up'
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(up_snapshot.0, "clob_snapshot");
    assert_eq!(up_snapshot.1, "mkt-1");
    assert_eq!(up_snapshot.2, "tok-up");
    assert_eq!(up_snapshot.3, "legacy_snapshot");
}

#[test]
/// Verify that legacy trade-audit fields are populated from historical trades.
fn backfill_trade_audit_populates_legacy_execution_fields() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    conn.execute(
        "INSERT INTO markets (
            market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time,
            status, fee_profile
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'resolved', 'crypto')",
        params![
            "mkt-1",
            "Will BTC go up?",
            "cond-1",
            "btc-updown-5m",
            "tok-up",
            "tok-down",
            1_000_i64,
            2_000_000_000_000_i64
        ],
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO signals (
            id, timestamp, strategy, direction, binance_price, chainlink_price,
            up_ask, down_ask, up_bid, down_bid, metadata, market_id
         ) VALUES
            (11, 1500, 'spread-capture', 'UP', 68000.0, 68010.0, 0.48, 0.49, 0.47, 0.48, '{}', 'mkt-1'),
            (12, 1500, 'spread-capture', 'DOWN', 68000.0, 68010.0, 0.48, 0.49, 0.47, 0.48, '{}', 'mkt-1');

         INSERT INTO simulated_trades (
            id, timestamp, market_id, strategy, side, token_id, entry_price, size, status
         ) VALUES
            (1, 1500, 'mkt-1', 'spread-capture', 'UP', 'tok-up', 0.48, 20.0, 'closed'),
            (2, 1500, 'mkt-1', 'spread-capture', 'DOWN', 'tok-down', 0.49, 20.0, 'closed');

         INSERT INTO trade_results (
            trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at
         ) VALUES
            (1, 1.0, 1.0, 10.0, 9.0, 8.0, 7.0, 2000),
            (2, 0.0, 0.0, -9.0, -10.0, -11.0, -12.0, 2000);",
    )
    .unwrap();

    backfill_trade_audit(&conn).unwrap();

    let rows = conn
        .prepare(
            "SELECT signal_id, fill_status, fill_reason, execution_group_id, execution_fidelity
             FROM simulated_trades
             ORDER BY id",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows[0].0, Some(11));
    assert_eq!(rows[1].0, Some(12));
    assert_eq!(rows[0].1, "legacy_assumed_full");
    assert_eq!(rows[0].2, "snapshot_backfill");
    assert_eq!(rows[0].3, rows[1].3);
    assert!(rows[0].3.as_deref().unwrap().starts_with("legacy-spread-"));
    assert_eq!(rows[0].4, "legacy_snapshot");

    let fee_updates = conn
        .prepare(
            "SELECT fee_amount, pnl_net, settlement_status FROM trade_results ORDER BY trade_id",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert!(fee_updates[0].0 > 0.0);
    assert!(fee_updates[0].1 < 10.0);
    assert_eq!(fee_updates[0].2, "confirmed");
}

#[test]
/// Verify that Gamma metadata selection prefers the matching market entry.
fn extract_gamma_metadata_prefers_matching_market_and_parses_numeric_strings() {
    let body = serde_json::json!({
        "resolutionSource": "global-fallback",
        "markets": [
            {
                "question": "Other market",
                "clobTokenIds": ["x", "y"],
                "orderMinSize": "5",
                "orderPriceMinTickSize": "0.05"
            },
            {
                "question": "Will BTC go up?",
                "clobTokenIds": ["tok-up", "tok-down"],
                "resolutionSource": "chainlink",
                "orderMinSize": "10",
                "orderPriceMinTickSize": "0.01",
                "makerBaseFee": "0.0",
                "takerBaseFee": "0.072",
                "rewardsMinSize": "50",
                "rewardsMaxSpread": "0.03"
            }
        ]
    });

    let meta = extract_gamma_metadata(&body, "Will BTC go up?", "tok-up", "tok-down");
    assert_eq!(meta.resolution_source.as_deref(), Some("chainlink"));
    assert_eq!(meta.order_min_size, Some(10.0));
    assert_eq!(meta.order_price_min_tick_size, Some(0.01));
    assert_eq!(meta.taker_base_fee, Some(0.072));
    assert_eq!(meta.rewards_min_size, Some(50.0));
    assert_eq!(meta.rewards_max_spread, Some(0.03));
}
