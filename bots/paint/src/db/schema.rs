/// SQL migration statements executed once in order when the database is created.
pub const MIGRATIONS: &[&str] = &[
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
    run_base_migrations(conn)?;
    ensure_legacy_columns(conn);
    ensure_additive_tables(conn);
    ensure_feed_event_columns(conn);
    ensure_signal_metric_columns(conn);

    Ok(())
}

/// Apply the static SQL migration list inside one transaction.
fn run_base_migrations(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for sql in MIGRATIONS {
        tx.execute_batch(sql)?;
    }
    tx.commit()
}

/// Add all legacy compatibility columns required by the modern runtime.
fn ensure_legacy_columns(conn: &rusqlite::Connection) {
    add_column_if_missing(conn, "markets", "outcome", "TEXT");
    add_column_if_missing(conn, "markets", "polymarket_outcome", "TEXT");
    add_column_if_missing(conn, "markets", "resolution_source", "TEXT");
    add_column_if_missing(conn, "markets", "fee_profile", "TEXT");
    add_column_if_missing(conn, "markets", "order_min_size", "REAL");
    add_column_if_missing(conn, "markets", "order_price_min_tick_size", "REAL");
    add_column_if_missing(conn, "markets", "maker_base_fee", "REAL");
    add_column_if_missing(conn, "markets", "taker_base_fee", "REAL");
    add_column_if_missing(conn, "markets", "rewards_min_size", "REAL");
    add_column_if_missing(conn, "markets", "rewards_max_spread", "REAL");
    add_column_if_missing(conn, "markets", "fees_enabled", "INTEGER");
    add_column_if_missing(conn, "markets", "fee_schedule_json", "TEXT");
    add_column_if_missing(conn, "markets", "token_fee_rates_json", "TEXT");
    add_column_if_missing(conn, "markets", "accepting_orders", "INTEGER");
    add_column_if_missing(conn, "markets", "accepting_orders_timestamp", "TEXT");
    add_column_if_missing(conn, "markets", "clear_book_on_start", "INTEGER");
    add_column_if_missing(conn, "signals", "market_id", "TEXT");
    add_column_if_missing(conn, "signals", "execution_fidelity", "TEXT");
    add_column_if_missing(conn, "signals", "strategy_version", "TEXT DEFAULT 'v1'");
    add_column_if_missing(
        conn,
        "signals",
        "feature_mode",
        "TEXT DEFAULT 'legacy_core'",
    );
    add_column_if_missing(conn, "signals", "order_submitted_at_ms", "INTEGER");
    add_column_if_missing(conn, "signals", "order_arrival_at_ms", "INTEGER");
    add_column_if_missing(conn, "trade_results", "fee_amount", "REAL DEFAULT 0");
    add_column_if_missing(conn, "trade_results", "pnl_net", "REAL DEFAULT 0");
    add_column_if_missing(
        conn,
        "trade_results",
        "settlement_status",
        "TEXT DEFAULT 'confirmed'",
    );
    add_column_if_missing(conn, "trade_results", "provisional_pnl", "REAL");

    add_column_if_missing(
        conn,
        "simulated_trades",
        "execution_mode",
        "TEXT DEFAULT 'paper'",
    );
    add_column_if_missing(conn, "simulated_trades", "signal_id", "INTEGER");
    add_column_if_missing(conn, "simulated_trades", "order_id", "TEXT");
    add_column_if_missing(conn, "simulated_trades", "fill_price", "REAL");
    add_column_if_missing(conn, "simulated_trades", "requested_price", "REAL");
    add_column_if_missing(conn, "simulated_trades", "requested_size", "REAL");
    add_column_if_missing(conn, "simulated_trades", "filled_size", "REAL");
    add_column_if_missing(conn, "simulated_trades", "avg_fill_price", "REAL");
    add_column_if_missing(conn, "simulated_trades", "fill_status", "TEXT");
    add_column_if_missing(conn, "simulated_trades", "fill_reason", "TEXT");
    add_column_if_missing(conn, "simulated_trades", "fill_latency_ms", "INTEGER");
    add_column_if_missing(conn, "simulated_trades", "execution_group_id", "TEXT");
    add_column_if_missing(conn, "simulated_trades", "execution_fidelity", "TEXT");
}

/// Create additive runtime tables required by upgraded historical runs.
#[allow(clippy::too_many_lines)]
fn ensure_additive_tables(conn: &rusqlite::Connection) {
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

    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feed_events (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            received_at_ms  INTEGER NOT NULL,
            event_at_ms     INTEGER NOT NULL,
            received_at_us  INTEGER,
            event_at_us     INTEGER,
            source          TEXT NOT NULL,
            event_type      TEXT NOT NULL,
            source_topic    TEXT,
            source_symbol   TEXT,
            connection_id   TEXT,
            sequence_key    TEXT,
            market_id       TEXT,
            asset_id        TEXT,
            price           REAL,
            trade_size      REAL,
            signed_quantity REAL,
            best_bid        REAL,
            best_ask        REAL,
            bid_size        REAL,
            ask_size        REAL,
            depth_bid_notional REAL,
            depth_ask_notional REAL,
            depth_imbalance REAL,
            microprice      REAL,
            payload_json    TEXT,
            details_json    TEXT,
            fidelity        TEXT NOT NULL DEFAULT 'raw_event'
                CHECK(fidelity IN ('raw_event','legacy_snapshot'))
        );
        CREATE INDEX IF NOT EXISTS idx_feed_events_received ON feed_events(received_at_ms);
        CREATE INDEX IF NOT EXISTS idx_feed_events_received_us ON feed_events(received_at_us);
        CREATE INDEX IF NOT EXISTS idx_feed_events_source_ts ON feed_events(source, received_at_ms);
        CREATE INDEX IF NOT EXISTS idx_feed_events_market_ts ON feed_events(market_id, received_at_ms);
        CREATE TABLE IF NOT EXISTS run_metadata (
            key             TEXT PRIMARY KEY,
            value           TEXT NOT NULL,
            recorded_at_ms  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_signals_market_ts ON signals(market_id, timestamp);
        CREATE INDEX IF NOT EXISTS idx_trades_signal ON simulated_trades(signal_id);
        CREATE INDEX IF NOT EXISTS idx_trades_group ON simulated_trades(execution_group_id);
        CREATE TABLE IF NOT EXISTS signal_metrics (
            signal_id INTEGER PRIMARY KEY,
            generated_at_ms INTEGER NOT NULL,
            generated_at_us INTEGER,
            order_submitted_at_ms INTEGER,
            order_submitted_at_us INTEGER,
            expected_arrival_at_ms INTEGER,
            expected_arrival_at_us INTEGER,
            order_processed_at_ms INTEGER,
            order_processed_at_us INTEGER,
            effective_arrival_delay_ms INTEGER,
            binance_age_ms INTEGER,
            chainlink_age_ms INTEGER,
            clob_age_ms INTEGER,
            quote_age_ms INTEGER,
            book_staleness_ms INTEGER,
            expected_fee REAL,
            expected_slippage REAL,
            expected_edge REAL,
            available_feature_count INTEGER NOT NULL DEFAULT 0,
            decision_status TEXT NOT NULL DEFAULT 'generated',
            rejection_reason TEXT,
            features_json TEXT NOT NULL,
            FOREIGN KEY (signal_id) REFERENCES signals(id)
        );
        CREATE TABLE IF NOT EXISTS feed_health_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp_ms INTEGER NOT NULL,
            timestamp_us INTEGER,
            source TEXT NOT NULL,
            event_type TEXT NOT NULL,
            connection_id TEXT,
            market_id TEXT,
            details_json TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_feed_health_source_ts ON feed_health_events(source, timestamp_ms);
        CREATE TABLE IF NOT EXISTS strategy_rejection_summaries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp_ms INTEGER NOT NULL,
            market_id TEXT NOT NULL,
            strategy TEXT NOT NULL,
            reason TEXT NOT NULL,
            count INTEGER NOT NULL,
            details_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_strategy_rejections_market_ts ON strategy_rejection_summaries(market_id, timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_strategy_rejections_reason_ts ON strategy_rejection_summaries(reason, timestamp_ms);
        CREATE TABLE IF NOT EXISTS history_upgrades (
            key TEXT PRIMARY KEY,
            completed_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS live_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at_ms INTEGER NOT NULL,
            ended_at_ms INTEGER,
            status TEXT NOT NULL,
            execution_mode TEXT NOT NULL,
            wallet_address TEXT,
            proxy_wallet TEXT,
            enabled_strategies_json TEXT NOT NULL,
            config_fingerprint TEXT NOT NULL,
            cash_cap_usd REAL NOT NULL,
            details_json TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_live_sessions_started ON live_sessions(started_at_ms);
        CREATE TABLE IF NOT EXISTS live_order_intents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            signal_id INTEGER,
            market_id TEXT NOT NULL,
            strategy TEXT NOT NULL,
            side TEXT NOT NULL,
            order_type TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            requested_price REAL,
            requested_size REAL,
            limit_price REAL,
            fee_schedule_json TEXT,
            token_fee_rates_json TEXT,
            execution_group_id TEXT,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id),
            FOREIGN KEY (signal_id) REFERENCES signals(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_order_intents_session_ts ON live_order_intents(session_id, created_at_ms);
        CREATE INDEX IF NOT EXISTS idx_live_order_intents_market_ts ON live_order_intents(market_id, created_at_ms);
        CREATE TABLE IF NOT EXISTS live_orders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            intent_id INTEGER NOT NULL,
            venue_order_id TEXT,
            client_order_id TEXT,
            market_id TEXT NOT NULL,
            token_id TEXT,
            side TEXT NOT NULL,
            order_type TEXT NOT NULL,
            status TEXT NOT NULL,
            status_reason TEXT,
            created_at_ms INTEGER NOT NULL,
            acknowledged_at_ms INTEGER,
            updated_at_ms INTEGER NOT NULL,
            requested_price REAL,
            limit_price REAL,
            requested_size REAL,
            accepted_size REAL,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id),
            FOREIGN KEY (intent_id) REFERENCES live_order_intents(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_orders_session_ts ON live_orders(session_id, updated_at_ms);
        CREATE INDEX IF NOT EXISTS idx_live_orders_status_ts ON live_orders(status, updated_at_ms);
        CREATE TABLE IF NOT EXISTS live_fills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            intent_id INTEGER,
            live_order_id INTEGER,
            venue_trade_id TEXT,
            filled_at_ms INTEGER NOT NULL,
            price REAL NOT NULL,
            size REAL NOT NULL,
            fee_amount REAL,
            fee_rate REAL,
            liquidity_side TEXT,
            tx_hash TEXT,
            status TEXT NOT NULL,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id),
            FOREIGN KEY (intent_id) REFERENCES live_order_intents(id),
            FOREIGN KEY (live_order_id) REFERENCES live_orders(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_fills_session_ts ON live_fills(session_id, filled_at_ms);
        CREATE TABLE IF NOT EXISTS live_account_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            cash_available REAL NOT NULL,
            cash_reserved_for_orders REAL NOT NULL,
            inventory_mark_value REAL NOT NULL,
            redeemable_value REAL NOT NULL,
            pending_redeem_value REAL NOT NULL,
            total_equity REAL NOT NULL,
            allowance_available REAL,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_account_snapshots_session_ts ON live_account_snapshots(session_id, timestamp_ms);
        CREATE TABLE IF NOT EXISTS live_redemptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            market_id TEXT NOT NULL,
            detected_redeemable_at_ms INTEGER NOT NULL,
            submitted_at_ms INTEGER,
            confirmed_at_ms INTEGER,
            cash_credit_observed_at_ms INTEGER,
            status TEXT NOT NULL,
            redeemable_value REAL NOT NULL,
            tx_hash TEXT,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_redemptions_session_ts ON live_redemptions(session_id, detected_redeemable_at_ms);
        CREATE TABLE IF NOT EXISTS live_reconciliation_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            severity TEXT NOT NULL,
            event_type TEXT NOT NULL,
            local_value REAL,
            remote_value REAL,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_reconciliation_session_ts ON live_reconciliation_events(session_id, timestamp_ms);
        CREATE TABLE IF NOT EXISTS control_audit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp_ms INTEGER NOT NULL,
            actor TEXT NOT NULL,
            action TEXT NOT NULL,
            target TEXT,
            details_json TEXT
        );
        CREATE TABLE IF NOT EXISTS live_control_state (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            state TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            actor TEXT NOT NULL,
            reason TEXT NOT NULL,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_live_control_state_session_ts ON live_control_state(session_id, updated_at_ms);
        CREATE TABLE IF NOT EXISTS live_control_commands (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            action TEXT NOT NULL,
            actor TEXT NOT NULL,
            reason TEXT NOT NULL,
            requested_at_ms INTEGER NOT NULL,
            applied_at_ms INTEGER,
            status TEXT NOT NULL,
            details_json TEXT,
            FOREIGN KEY (session_id) REFERENCES live_sessions(id)
        );",
    );
}

/// Add any post-creation `feed_events` columns required by newer runs.
fn ensure_feed_event_columns(conn: &rusqlite::Connection) {
    add_column_if_missing(conn, "feed_events", "received_at_us", "INTEGER");
    add_column_if_missing(conn, "feed_events", "event_at_us", "INTEGER");
    add_column_if_missing(conn, "feed_events", "source_topic", "TEXT");
    add_column_if_missing(conn, "feed_events", "source_symbol", "TEXT");
    add_column_if_missing(conn, "feed_events", "connection_id", "TEXT");
    add_column_if_missing(conn, "feed_events", "sequence_key", "TEXT");
    add_column_if_missing(conn, "feed_events", "details_json", "TEXT");
    add_column_if_missing(conn, "feed_events", "trade_size", "REAL");
    add_column_if_missing(conn, "feed_events", "signed_quantity", "REAL");
    add_column_if_missing(conn, "feed_events", "depth_bid_notional", "REAL");
    add_column_if_missing(conn, "feed_events", "depth_ask_notional", "REAL");
    add_column_if_missing(conn, "feed_events", "depth_imbalance", "REAL");
    add_column_if_missing(conn, "feed_events", "microprice", "REAL");
}

/// Add any post-creation `signal_metrics` columns required by newer runs.
fn ensure_signal_metric_columns(conn: &rusqlite::Connection) {
    add_column_if_missing(conn, "signal_metrics", "order_processed_at_ms", "INTEGER");
    add_column_if_missing(conn, "signal_metrics", "order_processed_at_us", "INTEGER");
    add_column_if_missing(
        conn,
        "signal_metrics",
        "effective_arrival_delay_ms",
        "INTEGER",
    );
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

#[cfg(test)]
#[path = "tests/schema_tests.rs"]
mod tests;
