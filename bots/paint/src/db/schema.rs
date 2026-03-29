/// SQL migration statements — executed once in order when the database is created.
///
/// Ported verbatim from the TypeScript `MIGRATIONS` array.
pub const MIGRATIONS: &[&str] = &[
    // -- tick_data ------------------------------------------------------------
    "CREATE TABLE IF NOT EXISTS tick_data (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    source    TEXT NOT NULL CHECK(source IN ('binance','clob_up','clob_down','chainlink')),
    price     REAL,
    bid       REAL,
    ask       REAL,
    bid_size  REAL,
    ask_size  REAL
  )",
    "CREATE INDEX IF NOT EXISTS idx_tick_ts ON tick_data(timestamp)",
    "CREATE INDEX IF NOT EXISTS idx_tick_source_ts ON tick_data(source, timestamp)",
    // -- markets --------------------------------------------------------------
    "CREATE TABLE IF NOT EXISTS markets (
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
      CHECK(status IN ('active','closed','resolved'))
  )",
    "CREATE INDEX IF NOT EXISTS idx_markets_end ON markets(end_time)",
    "CREATE INDEX IF NOT EXISTS idx_markets_status ON markets(status)",
    // -- signals --------------------------------------------------------------
    "CREATE TABLE IF NOT EXISTS signals (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       INTEGER NOT NULL,
    strategy        TEXT NOT NULL,
    direction       TEXT NOT NULL CHECK(direction IN ('UP','DOWN')),
    binance_price   REAL,
    chainlink_price REAL,
    up_ask          REAL,
    down_ask        REAL,
    up_bid          REAL,
    down_bid        REAL,
    metadata        TEXT
  )",
    "CREATE INDEX IF NOT EXISTS idx_signals_ts ON signals(timestamp)",
    "CREATE INDEX IF NOT EXISTS idx_signals_strat_ts ON signals(strategy, timestamp)",
    // -- simulated_trades -----------------------------------------------------
    "CREATE TABLE IF NOT EXISTS simulated_trades (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   INTEGER NOT NULL,
    market_id   TEXT NOT NULL,
    strategy    TEXT NOT NULL,
    side        TEXT NOT NULL CHECK(side IN ('UP','DOWN')),
    token_id    TEXT NOT NULL,
    entry_price REAL NOT NULL,
    size        REAL NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open'
      CHECK(status IN ('open','closed','expired')),
    FOREIGN KEY (market_id) REFERENCES markets(market_id)
  )",
    "CREATE INDEX IF NOT EXISTS idx_trades_status ON simulated_trades(status)",
    "CREATE INDEX IF NOT EXISTS idx_trades_market ON simulated_trades(market_id)",
    // -- trade_results --------------------------------------------------------
    "CREATE TABLE IF NOT EXISTS trade_results (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    trade_id         INTEGER NOT NULL UNIQUE,
    exit_price       REAL,
    settlement_price REAL NOT NULL,
    pnl_0pct         REAL NOT NULL,
    pnl_1pct         REAL NOT NULL,
    pnl_2pct         REAL NOT NULL,
    pnl_3pct         REAL NOT NULL,
    resolved_at      INTEGER NOT NULL,
    FOREIGN KEY (trade_id) REFERENCES simulated_trades(id)
  )",
    // -- balance_log ----------------------------------------------------------
    "CREATE TABLE IF NOT EXISTS balance_log (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    event     TEXT NOT NULL,
    trade_id  INTEGER,
    amount    REAL NOT NULL,
    balance   REAL NOT NULL,
    FOREIGN KEY (trade_id) REFERENCES simulated_trades(id)
  )",
    "CREATE INDEX IF NOT EXISTS idx_balance_ts ON balance_log(timestamp)",
];

/// Apply all migrations inside a single transaction.
pub fn run_migrations(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for sql in MIGRATIONS {
        tx.execute_batch(sql)?;
    }
    tx.commit()?;

    // v0.7 schema additions (idempotent, outside the main migration array
    // so they can safely fail on "duplicate column" for existing databases).
    add_column_if_missing(conn, "markets", "outcome", "TEXT");
    add_column_if_missing(conn, "trade_results", "fee_amount", "REAL DEFAULT 0");
    add_column_if_missing(conn, "trade_results", "pnl_net", "REAL DEFAULT 0");
    add_column_if_missing(
        conn,
        "trade_results",
        "settlement_status",
        "TEXT DEFAULT 'confirmed'",
    );
    add_column_if_missing(conn, "trade_results", "provisional_pnl", "REAL");
    // Future live trading columns.
    add_column_if_missing(
        conn,
        "simulated_trades",
        "execution_mode",
        "TEXT DEFAULT 'paper'",
    );
    add_column_if_missing(conn, "simulated_trades", "order_id", "TEXT");
    add_column_if_missing(conn, "simulated_trades", "fill_price", "REAL");

    // Settlement audit trail (v0.7).
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settlement_audit (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            trade_id            INTEGER NOT NULL,
            market_id           TEXT NOT NULL,
            our_prediction      TEXT NOT NULL,
            polymarket_outcome  TEXT NOT NULL,
            matched             INTEGER NOT NULL,
            timestamp           INTEGER NOT NULL,
            FOREIGN KEY (trade_id) REFERENCES simulated_trades(id)
        )",
    );

    Ok(())
}

/// Add a column to a table if it does not already exist.
fn add_column_if_missing(conn: &rusqlite::Connection, table: &str, column: &str, col_type: &str) {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {col_type}");
    match conn.execute_batch(&sql) {
        Ok(()) => {}
        Err(e) if e.to_string().contains("duplicate column") => {}
        Err(e) => tracing::warn!("failed to add column {table}.{column}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/schema_tests.rs"]
mod tests;
