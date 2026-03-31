use super::*;
use crate::db::schema::run_migrations;
use crate::types::ReplayFidelity;

/// Helper: create an in-memory DB with the full schema.
fn setup_test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Helper: insert a tick row.
#[allow(clippy::too_many_arguments)]
fn insert_tick(
    conn: &rusqlite::Connection,
    timestamp: i64,
    source: &str,
    price: Option<f64>,
    bid: Option<f64>,
    ask: Option<f64>,
    bid_size: Option<f64>,
    ask_size: Option<f64>,
) {
    conn.execute(
        "INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![timestamp, source, price, bid, ask, bid_size, ask_size],
    )
    .unwrap();
}

/// Helper: build a raw tick for in-memory tests.
fn tick(ts: u64, source: &str, price: Option<f64>) -> RawTick {
    RawTick {
        timestamp: ts,
        source: source.to_string(),
        event_type: "legacy_snapshot".to_string(),
        market_id: None,
        asset_id: None,
        price,
        bid: None,
        ask: None,
        bid_size: None,
        ask_size: None,
        fidelity: ReplayFidelity::LegacySnapshot,
    }
}

/// Verifies that empty replay returns none.
#[test]
fn empty_replay_returns_none() {
    let mut replay = TickReplay::from_cached(vec![]);
    assert_eq!(replay.total_ticks(), 0);
    assert!(replay.next_group().is_none());
}

/// Verifies that single tick returns one group.
#[test]
fn single_tick_returns_one_group() {
    let mut replay = TickReplay::from_cached(vec![tick(1000, "binance", Some(42_000.0))]);
    assert_eq!(replay.total_ticks(), 1);

    let group = replay.next_group().unwrap();
    assert_eq!(group.timestamp, 1000);
    assert!(group.binance.is_some());
    assert!(group.chainlink.is_none());
    assert!(group.clob_up.is_none());
    assert!(group.clob_down.is_none());
    assert_eq!(group.binance.unwrap().price, Some(42_000.0));

    assert!(replay.next_group().is_none());
}

/// Verifies that ticks within 10ms are grouped.
#[test]
fn ticks_within_10ms_are_grouped() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1005, "clob_up", None),
        tick(1010, "clob_down", None),
    ];
    let mut replay = TickReplay::from_cached(ticks);
    let group = replay.next_group().unwrap();
    assert_eq!(group.timestamp, 1000);
    assert!(group.binance.is_some());
    assert!(group.clob_up.is_some());
    assert!(group.clob_down.is_some());
    assert!(replay.next_group().is_none());
}

/// Verifies that ticks beyond 10ms are separate groups.
#[test]
fn ticks_beyond_10ms_are_separate_groups() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1011, "chainlink", Some(42_001.0)),
    ];
    let mut replay = TickReplay::from_cached(ticks);

    let g1 = replay.next_group().unwrap();
    assert_eq!(g1.timestamp, 1000);
    assert!(g1.binance.is_some());
    assert!(g1.chainlink.is_none());

    let g2 = replay.next_group().unwrap();
    assert_eq!(g2.timestamp, 1011);
    assert!(g2.chainlink.is_some());
    assert!(g2.binance.is_none());

    assert!(replay.next_group().is_none());
}

/// Verifies that boundary at exactly 10ms.
#[test]
fn boundary_at_exactly_10ms() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1010, "chainlink", Some(41_999.0)),
    ];
    let mut replay = TickReplay::from_cached(ticks);
    let group = replay.next_group().unwrap();
    assert!(group.binance.is_some());
    assert!(group.chainlink.is_some());
    assert!(replay.next_group().is_none());
}

/// Verifies that total ticks count.
#[test]
fn total_ticks_count() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(2000, "binance", Some(42_001.0)),
    ];
    let replay = TickReplay::from_cached(ticks);
    assert_eq!(replay.total_ticks(), 2);
}

/// Verifies that reset replays from start.
#[test]
fn reset_replays_from_start() {
    let mut replay = TickReplay::from_cached(vec![tick(1000, "binance", Some(42_000.0))]);
    let _ = replay.next_group();
    assert!(replay.next_group().is_none());
    replay.reset();
    assert!(replay.next_group().is_some());
}

/// Verifies that later source overwrites earlier in same group.
#[test]
fn later_source_overwrites_earlier_in_same_group() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1005, "binance", Some(42_100.0)),
    ];
    let mut replay = TickReplay::from_cached(ticks);
    let group = replay.next_group().unwrap();
    assert_eq!(group.binance.unwrap().price, Some(42_100.0));
}

/// Verifies that multiple groups sequential.
#[test]
fn multiple_groups_sequential() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1020, "binance", Some(42_100.0)),
        tick(1040, "binance", Some(42_200.0)),
    ];
    let mut replay = TickReplay::from_cached(ticks);

    let g1 = replay.next_group().unwrap();
    assert_eq!(g1.timestamp, 1000);
    assert_eq!(g1.binance.unwrap().price, Some(42_000.0));

    let g2 = replay.next_group().unwrap();
    assert_eq!(g2.timestamp, 1020);
    assert_eq!(g2.binance.unwrap().price, Some(42_100.0));

    let g3 = replay.next_group().unwrap();
    assert_eq!(g3.timestamp, 1040);
    assert_eq!(g3.binance.unwrap().price, Some(42_200.0));

    assert!(replay.next_group().is_none());
}

/// Verifies that all four sources grouped.
#[test]
fn all_four_sources_grouped() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1003, "chainlink", Some(41_999.0)),
        RawTick {
            timestamp: 1006,
            source: "clob_up".into(),
            event_type: "legacy_snapshot".into(),
            market_id: None,
            asset_id: None,
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: Some(100.0),
            ask_size: Some(200.0),
            fidelity: ReplayFidelity::LegacySnapshot,
        },
        RawTick {
            timestamp: 1009,
            source: "clob_down".into(),
            event_type: "legacy_snapshot".into(),
            market_id: None,
            asset_id: None,
            price: None,
            bid: Some(0.40),
            ask: Some(0.50),
            bid_size: Some(50.0),
            ask_size: Some(75.0),
            fidelity: ReplayFidelity::LegacySnapshot,
        },
    ];
    let mut replay = TickReplay::from_cached(ticks);
    let group = replay.next_group().unwrap();
    assert!(group.binance.is_some());
    assert!(group.chainlink.is_some());
    assert!(group.clob_up.is_some());
    assert!(group.clob_down.is_some());

    let clob_up = group.clob_up.unwrap();
    assert_eq!(clob_up.bid, Some(0.45));
    assert_eq!(clob_up.ask, Some(0.55));
    assert_eq!(clob_up.bid_size, Some(100.0));
    assert_eq!(clob_up.ask_size, Some(200.0));

    assert!(replay.next_group().is_none());
}

/// Verifies that from db empty returns no groups.
#[test]
fn from_db_empty_returns_no_groups() {
    let conn = setup_test_db();
    let mut replay = TickReplay::from_db(&conn, 0, 100_000).unwrap();
    assert_eq!(replay.total_ticks(), 0);
    assert!(replay.next_group().is_none());
}

/// Verifies that from db single binance tick.
#[test]
fn from_db_single_binance_tick() {
    let conn = setup_test_db();
    insert_tick(
        &conn,
        1000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    );

    let mut replay = TickReplay::from_db(&conn, 0, 2000).unwrap();
    assert_eq!(replay.total_ticks(), 1);

    let group = replay.next_group().unwrap();
    assert_eq!(group.timestamp, 1000);
    assert!(group.binance.is_some());
    assert_eq!(group.binance.unwrap().price, Some(42_000.0));
    assert!(replay.next_group().is_none());
}

/// Verifies that from db groups within 10ms.
#[test]
fn from_db_groups_within_10ms() {
    let conn = setup_test_db();
    insert_tick(
        &conn,
        1000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1005,
        "chainlink",
        Some(41_999.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1010,
        "clob_up",
        None,
        Some(0.45),
        Some(0.55),
        Some(100.0),
        Some(200.0),
    );

    let mut replay = TickReplay::from_db(&conn, 0, 2000).unwrap();
    assert_eq!(replay.total_ticks(), 3);

    let group = replay.next_group().unwrap();
    assert_eq!(group.timestamp, 1000);
    assert!(group.binance.is_some());
    assert!(group.chainlink.is_some());
    assert!(group.clob_up.is_some());
    assert!(group.clob_down.is_none());
    assert!(replay.next_group().is_none());
}

/// Verifies that from db splits beyond 10ms.
#[test]
fn from_db_splits_beyond_10ms() {
    let conn = setup_test_db();
    insert_tick(
        &conn,
        1000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1011,
        "chainlink",
        Some(41_999.0),
        None,
        None,
        None,
        None,
    );

    let mut replay = TickReplay::from_db(&conn, 0, 2000).unwrap();
    assert_eq!(replay.total_ticks(), 2);

    let g1 = replay.next_group().unwrap();
    assert_eq!(g1.timestamp, 1000);
    assert!(g1.binance.is_some());
    assert!(g1.chainlink.is_none());

    let g2 = replay.next_group().unwrap();
    assert_eq!(g2.timestamp, 1011);
    assert!(g2.chainlink.is_some());
    assert!(replay.next_group().is_none());
}

/// Verifies that from db time range filtering.
#[test]
fn from_db_time_range_filtering() {
    let conn = setup_test_db();
    insert_tick(
        &conn,
        500,
        "binance",
        Some(41_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1500,
        "binance",
        Some(43_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        2000,
        "binance",
        Some(44_000.0),
        None,
        None,
        None,
        None,
    );

    let mut replay = TickReplay::from_db(&conn, 1000, 1500).unwrap();
    assert_eq!(replay.total_ticks(), 2);

    let g1 = replay.next_group().unwrap();
    assert_eq!(g1.binance.unwrap().price, Some(42_000.0));
    let g2 = replay.next_group().unwrap();
    assert_eq!(g2.binance.unwrap().price, Some(43_000.0));
    assert!(replay.next_group().is_none());
}

/// Verifies that load ticks static method.
#[test]
fn load_ticks_static_method() {
    let conn = setup_test_db();
    insert_tick(
        &conn,
        1000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        2000,
        "chainlink",
        Some(41_999.0),
        None,
        None,
        None,
        None,
    );

    let ticks = TickReplay::load_ticks(&conn, 0, 3000).unwrap();
    assert_eq!(ticks.len(), 2);
    assert_eq!(ticks[0].source, "binance");
    assert_eq!(ticks[1].source, "chainlink");
}

/// Verifies that unknown source silently ignored.
#[test]
fn unknown_source_silently_ignored() {
    let ticks = vec![tick(1000, "unknown", Some(42_000.0))];
    let mut replay = TickReplay::from_cached(ticks);
    let group = replay.next_group().unwrap();
    assert_eq!(group.timestamp, 1000);
    assert!(
        group.binance.is_none(),
        "unknown source should not set binance"
    );
    assert!(
        group.chainlink.is_none(),
        "unknown source should not set chainlink"
    );
    assert!(
        group.clob_up.is_none(),
        "unknown source should not set clob_up"
    );
    assert!(
        group.clob_down.is_none(),
        "unknown source should not set clob_down"
    );
}

/// Verifies that from db all four sources.
#[test]
fn from_db_all_four_sources() {
    let conn = setup_test_db();
    insert_tick(
        &conn,
        1000,
        "binance",
        Some(42_000.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1003,
        "chainlink",
        Some(41_999.0),
        None,
        None,
        None,
        None,
    );
    insert_tick(
        &conn,
        1006,
        "clob_up",
        None,
        Some(0.45),
        Some(0.55),
        Some(100.0),
        Some(200.0),
    );
    insert_tick(
        &conn,
        1009,
        "clob_down",
        None,
        Some(0.40),
        Some(0.50),
        Some(50.0),
        Some(75.0),
    );

    let mut replay = TickReplay::from_db(&conn, 0, 2000).unwrap();
    let group = replay.next_group().unwrap();
    assert!(group.binance.is_some());
    assert!(group.chainlink.is_some());
    assert!(group.clob_up.is_some());
    assert!(group.clob_down.is_some());
    assert!(replay.next_group().is_none());
}
