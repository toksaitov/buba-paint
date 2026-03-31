use super::*;

/// Create an in-memory DB with the markets table extended with the
/// backtest-specific columns (`open_price`, `close_price`, `outcome`).
fn setup_test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    crate::db::schema::run_migrations(&conn).unwrap();

    let _ = conn.execute_batch(
        "ALTER TABLE markets ADD COLUMN open_price REAL;
         ALTER TABLE markets ADD COLUMN close_price REAL;
         ALTER TABLE markets ADD COLUMN outcome TEXT;",
    );
    conn
}

/// Insert a settled market window into the test DB.
fn insert_market(
    conn: &rusqlite::Connection,
    market_id: &str,
    start_time: i64,
    end_time: i64,
    open_price: f64,
    close_price: f64,
    outcome: &str,
) {
    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug, \
                              up_token_id, down_token_id, start_time, end_time, \
                              open_price, close_price, outcome, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'resolved')",
        params![
            market_id,
            format!("Will BTC go {outcome}?"),
            format!("cond-{market_id}"),
            format!("slug-{market_id}"),
            format!("up-{market_id}"),
            format!("down-{market_id}"),
            start_time,
            end_time,
            open_price,
            close_price,
            outcome,
        ],
    )
    .unwrap();
}

/// Helper: build a `MarketSettlement` for in-memory tests.
fn settlement(id: &str, start: u64, end: u64, outcome: SignalDirection) -> MarketSettlement {
    MarketSettlement {
        market_id: id.to_string(),
        question: "Will BTC go up?".to_string(),
        up_token_id: format!("up-{id}"),
        down_token_id: format!("down-{id}"),
        condition_id: format!("cond-{id}"),
        slug: format!("slug-{id}"),
        start_time: start,
        end_time: end,
        open_price: 42_000.0,
        close_price: 42_100.0,
        outcome,
        resolution_source: Some("chainlink".to_string()),
        fee_profile: Some("crypto".to_string()),
        order_min_size: Some(5.0),
        order_price_min_tick_size: Some(0.01),
        maker_base_fee: Some(1000.0),
        taker_base_fee: Some(1000.0),
        rewards_min_size: Some(50.0),
        rewards_max_spread: Some(4.5),
    }
}

/// Verifies that empty returns no events.
#[test]
fn empty_returns_no_events() {
    let mut wm = WindowManager::from_settlements(vec![]);
    assert_eq!(wm.total_windows(), 0);
    let ev = wm.advance(1000);
    assert!(ev.opened.is_none());
    assert!(ev.closed.is_none());
    assert!(wm.current.is_none());
}

/// Verifies that opens at start time.
#[test]
fn opens_at_start_time() {
    let mut wm =
        WindowManager::from_settlements(vec![settlement("m1", 1000, 2000, SignalDirection::Up)]);

    let ev = wm.advance(999);
    assert!(ev.opened.is_none());
    assert!(wm.current.is_none());

    let ev = wm.advance(1000);
    assert!(ev.opened.is_some());
    assert!(ev.closed.is_none());
    assert!(wm.current.is_some());
    assert_eq!(ev.opened.unwrap().market_id, "m1");
}

/// Verifies that closes at end time.
#[test]
fn closes_at_end_time() {
    let mut wm =
        WindowManager::from_settlements(vec![settlement("m1", 1000, 2000, SignalDirection::Up)]);
    wm.advance(1000);

    let ev = wm.advance(1999);
    assert!(ev.opened.is_none());
    assert!(ev.closed.is_none());
    assert!(wm.current.is_some());

    let ev = wm.advance(2000);
    assert!(ev.closed.is_some());
    assert_eq!(ev.closed.unwrap().market_id, "m1");
    assert!(wm.current.is_none());
}

/// Verifies that consecutive windows close and open.
#[test]
fn consecutive_windows_close_and_open() {
    let mut wm = WindowManager::from_settlements(vec![
        settlement("m1", 1000, 2000, SignalDirection::Up),
        settlement("m2", 2000, 3000, SignalDirection::Down),
    ]);
    wm.advance(1000);

    let ev = wm.advance(2000);
    assert!(ev.closed.is_some());
    assert_eq!(ev.closed.as_ref().unwrap().market_id, "m1");
    assert!(ev.opened.is_some());
    assert_eq!(ev.opened.as_ref().unwrap().market_id, "m2");
    assert_eq!(wm.current.as_ref().unwrap().market_id, "m2");
}

/// Verifies that walk through three windows.
#[test]
fn walk_through_three_windows() {
    let mut wm = WindowManager::from_settlements(vec![
        settlement("m1", 1000, 2000, SignalDirection::Up),
        settlement("m2", 3000, 4000, SignalDirection::Down),
        settlement("m3", 5000, 6000, SignalDirection::Up),
    ]);
    assert_eq!(wm.total_windows(), 3);

    let ev = wm.advance(1000);
    assert_eq!(ev.opened.unwrap().market_id, "m1");

    let ev = wm.advance(2000);
    assert_eq!(ev.closed.unwrap().market_id, "m1");
    assert!(ev.opened.is_none());

    let ev = wm.advance(3000);
    assert_eq!(ev.opened.unwrap().market_id, "m2");

    let ev = wm.advance(4000);
    assert_eq!(ev.closed.unwrap().market_id, "m2");

    let ev = wm.advance(5000);
    assert_eq!(ev.opened.unwrap().market_id, "m3");

    let ev = wm.advance(6000);
    assert_eq!(ev.closed.unwrap().market_id, "m3");
    assert!(wm.current.is_none());
}

/// Verifies that skip expired windows.
#[test]
fn skip_expired_windows() {
    let mut wm = WindowManager::from_settlements(vec![
        settlement("m1", 1000, 2000, SignalDirection::Up),
        settlement("m2", 3000, 4000, SignalDirection::Down),
    ]);

    let ev = wm.advance(2500);
    assert!(ev.opened.is_none());
    assert!(ev.closed.is_none());

    let ev = wm.advance(3000);
    assert!(ev.opened.is_some());
    assert_eq!(ev.opened.unwrap().market_id, "m2");
}

/// Verifies that to market window converts.
#[test]
fn to_market_window_converts() {
    let s = settlement("m1", 1000, 2000, SignalDirection::Up);
    let mw = WindowManager::to_market_window(&s);
    assert_eq!(mw.market_id, "m1");
    assert_eq!(mw.question, "Will BTC go up?");
    assert_eq!(mw.up_token_id, "up-m1");
    assert_eq!(mw.down_token_id, "down-m1");
    assert_eq!(mw.condition_id, "cond-m1");
    assert_eq!(mw.slug, "slug-m1");
    assert_eq!(mw.start_time, 1000);
    assert_eq!(mw.end_time, 2000);
}

/// Verifies that reset allows replay.
#[test]
fn reset_allows_replay() {
    let mut wm =
        WindowManager::from_settlements(vec![settlement("m1", 1000, 2000, SignalDirection::Up)]);
    wm.advance(1000);
    assert!(wm.current.is_some());
    wm.advance(2000);
    assert!(wm.current.is_none());

    wm.reset();
    assert!(wm.current.is_none());

    let ev = wm.advance(1000);
    assert!(ev.opened.is_some());
    assert_eq!(ev.opened.unwrap().market_id, "m1");
}

/// Verifies that db empty returns zero windows.
#[test]
fn db_empty_returns_zero_windows() {
    let conn = setup_test_db();
    let wm = WindowManager::new(&conn, 0, 100_000).unwrap();
    assert_eq!(wm.total_windows(), 0);
    assert!(wm.current.is_none());
}

/// Verifies that db loads windows in range.
#[test]
fn db_loads_windows_in_range() {
    let conn = setup_test_db();
    insert_market(&conn, "m1", 1000, 2000, 42_000.0, 42_100.0, "UP");
    insert_market(&conn, "m2", 3000, 4000, 42_100.0, 42_050.0, "DOWN");
    insert_market(&conn, "m3", 5000, 6000, 42_050.0, 42_200.0, "UP");

    let wm = WindowManager::new(&conn, 0, 4000).unwrap();
    assert_eq!(wm.total_windows(), 2);
}

/// Verifies that db skips null outcome.
#[test]
fn db_skips_null_outcome() {
    let conn = setup_test_db();
    insert_market(&conn, "m1", 1000, 2000, 42_000.0, 42_100.0, "UP");
    conn.execute(
        "INSERT INTO markets (market_id, question, condition_id, slug, \
                              up_token_id, down_token_id, start_time, end_time, \
                              open_price, close_price, outcome, status) \
         VALUES ('m2', 'q', 'c', 's', 'u', 'd', 3000, 4000, 42000, 42100, NULL, 'active')",
        [],
    )
    .unwrap();

    let wm = WindowManager::new(&conn, 0, 5000).unwrap();
    assert_eq!(wm.total_windows(), 1);
}

/// Verifies that db opens and closes.
#[test]
fn db_opens_and_closes() {
    let conn = setup_test_db();
    insert_market(&conn, "m1", 1000, 2000, 42_000.0, 42_100.0, "UP");

    let mut wm = WindowManager::new(&conn, 0, 3000).unwrap();

    let ev = wm.advance(1000);
    assert!(ev.opened.is_some());
    let opened = ev.opened.unwrap();
    assert_eq!(opened.market_id, "m1");
    assert_eq!(opened.outcome, SignalDirection::Up);
    assert!((opened.open_price - 42_000.0).abs() < f64::EPSILON);
    assert!((opened.close_price - 42_100.0).abs() < f64::EPSILON);

    let ev = wm.advance(2000);
    assert!(ev.closed.is_some());
    assert!(wm.current.is_none());
}

/// Verifies that db down outcome parsed.
#[test]
fn db_down_outcome_parsed() {
    let conn = setup_test_db();
    insert_market(&conn, "m1", 1000, 2000, 42_100.0, 42_000.0, "DOWN");

    let mut wm = WindowManager::new(&conn, 0, 3000).unwrap();
    let ev = wm.advance(1000);
    let opened = ev.opened.unwrap();
    assert_eq!(opened.outcome, SignalDirection::Down);
}
