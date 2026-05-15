use super::*;

/// Verifies that migrations run on fresh db.
#[test]
fn migrations_run_on_fresh_db() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
             ('tick_data','markets','signals','simulated_trades','trade_results','balance_log')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 6);
}

/// Verifies that migrations are idempotent.
#[test]
fn migrations_are_idempotent() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    run_migrations(&conn).unwrap();
}

/// Verifies that migration count.
#[test]
fn migration_count() {
    assert_eq!(MIGRATIONS.len(), 15);
}

/// Verifies that runtime migrations create compact CLOB replay storage.
#[test]
fn migrations_create_compact_clob_replay_table() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
             ('clob_replay_events', 'clob_replay_blocks')",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(count, 2);
}

/// Verifies that runtime migrations do not create sweep-only feed indexes.
#[test]
fn runtime_migrations_skip_replay_indexes() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    assert!(!has_replay_indexes(&conn).unwrap());
}

/// Verifies that offline replay preparation creates sweep indexes.
#[test]
fn create_replay_indexes_adds_offline_indexes() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    create_replay_indexes(&conn).unwrap();

    assert!(has_replay_indexes(&conn).unwrap());
}
