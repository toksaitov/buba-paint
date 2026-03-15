use super::*;
use crate::db::schema::run_migrations;

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
        price,
        bid: None,
        ask: None,
        bid_size: None,
        ask_size: None,
    }
}

// -- from_cached / basic iteration ----------------------------------------

#[test]
fn empty_replay_returns_none() {
    let mut replay = TickReplay::from_cached(vec![]);
    assert_eq!(replay.total_ticks(), 0);
    assert!(replay.next_group().is_none());
}

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

#[test]
fn boundary_at_exactly_10ms() {
    // Difference is exactly 10: should be grouped (10 <= 10).
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

#[test]
fn total_ticks_count() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(2000, "binance", Some(42_001.0)),
    ];
    let replay = TickReplay::from_cached(ticks);
    assert_eq!(replay.total_ticks(), 2);
}

#[test]
fn reset_replays_from_start() {
    let mut replay = TickReplay::from_cached(vec![tick(1000, "binance", Some(42_000.0))]);
    let _ = replay.next_group();
    assert!(replay.next_group().is_none());
    replay.reset();
    assert!(replay.next_group().is_some());
}

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

#[test]
fn all_four_sources_grouped() {
    let ticks = vec![
        tick(1000, "binance", Some(42_000.0)),
        tick(1003, "chainlink", Some(41_999.0)),
        RawTick {
            timestamp: 1006,
            source: "clob_up".into(),
            price: None,
            bid: Some(0.45),
            ask: Some(0.55),
            bid_size: Some(100.0),
            ask_size: Some(200.0),
        },
        RawTick {
            timestamp: 1009,
            source: "clob_down".into(),
            price: None,
            bid: Some(0.40),
            ask: Some(0.50),
            bid_size: Some(50.0),
            ask_size: Some(75.0),
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

// -- from_db / load_ticks with in-memory SQLite ---------------------------

#[test]
fn from_db_empty_returns_no_groups() {
    let conn = setup_test_db();
    let mut replay = TickReplay::from_db(&conn, 0, 100_000).unwrap();
    assert_eq!(replay.total_ticks(), 0);
    assert!(replay.next_group().is_none());
}

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

#[test]
fn unknown_source_silently_ignored() {
    // A tick with source="unknown" should not populate any field in the group.
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
