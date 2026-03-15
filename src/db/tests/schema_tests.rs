use super::*;

#[test]
fn migrations_run_on_fresh_db() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    // Verify a few tables exist by querying sqlite_master
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

#[test]
fn migrations_are_idempotent() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    // Running a second time should not error (CREATE IF NOT EXISTS).
    run_migrations(&conn).unwrap();
}

#[test]
fn migration_count() {
    // Guard against accidentally removing a migration.
    assert_eq!(MIGRATIONS.len(), 15);
}
