use std::path::Path;

use anyhow::Context;
use rusqlite::{Connection, params};

struct RunInfo {
    number: u32,
    version: &'static str,
    subdir: &'static str,
}

const RUNS: &[RunInfo] = &[
    RunInfo {
        number: 4,
        version: "v0.2",
        subdir: "004",
    },
    RunInfo {
        number: 5,
        version: "v0.3",
        subdir: "005",
    },
    RunInfo {
        number: 6,
        version: "v0.4",
        subdir: "006",
    },
    RunInfo {
        number: 7,
        version: "v0.5",
        subdir: "007",
    },
    RunInfo {
        number: 8,
        version: "v0.6",
        subdir: "008",
    },
    RunInfo {
        number: 9,
        version: "v0.8.1",
        subdir: "009",
    },
];

const CREATE_SCHEMA: &str = "
    CREATE TABLE runs (
        id          INTEGER PRIMARY KEY,
        run_number  INTEGER NOT NULL UNIQUE,
        bot_version TEXT NOT NULL,
        start_time  INTEGER,
        end_time    INTEGER,
        total_trades INTEGER DEFAULT 0,
        win_rate    REAL DEFAULT 0
    );

    CREATE TABLE tick_data (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp INTEGER NOT NULL,
        source    TEXT NOT NULL,
        price     REAL,
        bid       REAL,
        ask       REAL,
        bid_size  REAL,
        ask_size  REAL,
        run_id    INTEGER NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs(id)
    );

    CREATE TABLE markets (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        market_id     TEXT NOT NULL UNIQUE,
        question      TEXT NOT NULL,
        condition_id  TEXT NOT NULL,
        slug          TEXT NOT NULL,
        up_token_id   TEXT NOT NULL,
        down_token_id TEXT NOT NULL,
        start_time    INTEGER NOT NULL,
        end_time      INTEGER NOT NULL,
        status        TEXT NOT NULL DEFAULT 'resolved',
        open_price    REAL,
        close_price   REAL,
        outcome       TEXT,
        polymarket_outcome TEXT,
        resolution_source TEXT,
        fee_profile   TEXT,
        order_min_size REAL,
        order_price_min_tick_size REAL,
        maker_base_fee REAL,
        taker_base_fee REAL,
        rewards_min_size REAL,
        rewards_max_spread REAL,
        run_id        INTEGER NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs(id)
    );

    CREATE TABLE feed_events (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        received_at_ms  INTEGER NOT NULL,
        event_at_ms     INTEGER NOT NULL,
        source          TEXT NOT NULL,
        event_type      TEXT NOT NULL,
        market_id       TEXT,
        asset_id        TEXT,
        price           REAL,
        best_bid        REAL,
        best_ask        REAL,
        bid_size        REAL,
        ask_size        REAL,
        payload_json    TEXT,
        fidelity        TEXT NOT NULL,
        run_id          INTEGER NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs(id)
    );

    CREATE TABLE data_quality (
        hour_start     INTEGER NOT NULL,
        source         TEXT NOT NULL,
        tick_count     INTEGER NOT NULL,
        expected_count INTEGER NOT NULL DEFAULT 3600,
        gap_count_5s   INTEGER NOT NULL DEFAULT 0,
        gap_count_30s  INTEGER NOT NULL DEFAULT 0,
        max_gap_ms     INTEGER NOT NULL DEFAULT 0,
        coverage_pct   REAL NOT NULL DEFAULT 0,
        PRIMARY KEY (hour_start, source)
    );

    CREATE TABLE historical_trades (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id           INTEGER NOT NULL,
        bot_version      TEXT NOT NULL,
        timestamp        INTEGER NOT NULL,
        strategy         TEXT NOT NULL,
        direction        TEXT NOT NULL,
        entry_price      REAL NOT NULL,
        size             REAL NOT NULL,
        settlement_price REAL NOT NULL,
        pnl              REAL NOT NULL,
        pnl_net          REAL NOT NULL,
        fee_amount       REAL NOT NULL,
        won              INTEGER NOT NULL,
        fill_status      TEXT,
        execution_group_id TEXT,
        execution_fidelity TEXT,
        FOREIGN KEY (run_id) REFERENCES runs(id)
    );
";

const CREATE_INDEXES: &str = "
    CREATE INDEX IF NOT EXISTS idx_tick_ts ON tick_data(timestamp);
    CREATE INDEX IF NOT EXISTS idx_tick_source_ts ON tick_data(source, timestamp);
    CREATE INDEX IF NOT EXISTS idx_tick_run ON tick_data(run_id);
    CREATE INDEX IF NOT EXISTS idx_markets_start ON markets(start_time);
    CREATE INDEX IF NOT EXISTS idx_markets_end ON markets(end_time);
    CREATE INDEX IF NOT EXISTS idx_markets_outcome ON markets(outcome);
    CREATE INDEX IF NOT EXISTS idx_markets_run ON markets(run_id);
    CREATE INDEX IF NOT EXISTS idx_feed_events_ts ON feed_events(received_at_ms);
    CREATE INDEX IF NOT EXISTS idx_feed_events_source_ts ON feed_events(source, received_at_ms);
    CREATE INDEX IF NOT EXISTS idx_feed_events_market_ts ON feed_events(market_id, received_at_ms);
    CREATE INDEX IF NOT EXISTS idx_htrades_run ON historical_trades(run_id);
";

/// Build a merged market-data database from individual run databases.
///
/// Source DBs are opened read-only via `ATTACH DATABASE ... ?mode=ro`.
/// The output file is deleted and rebuilt from scratch each time.
pub fn build_market_data(runs_dir: &str, output: &str) -> anyhow::Result<()> {
    log("Building merged market data DB...");
    log(&format!("Output: {output}"));

    if Path::new(output).exists() {
        std::fs::remove_file(output).context("removing existing output DB")?;
        log("Removed existing output DB");
    }

    if let Some(parent) = Path::new(output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory for output: {}", parent.display()))?;
        }
    }

    let conn = Connection::open(output).context("creating output database")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "cache_size", "-256000")?;

    conn.execute_batch(CREATE_SCHEMA)
        .context("creating schema")?;

    for run in RUNS {
        import_run(&conn, runs_dir, run)?;
    }

    log("Creating indexes...");
    conn.execute_batch(CREATE_INDEXES)
        .context("creating indexes")?;

    compute_settlements(&conn)?;

    compute_data_quality(&conn)?;

    print_summary(&conn)?;

    log("\nVACUUMing...");
    conn.execute_batch("VACUUM")?;

    log("\nDone!");
    Ok(())
}

/// Import run.
#[allow(clippy::too_many_lines)]
fn import_run(conn: &Connection, runs_dir: &str, run: &RunInfo) -> anyhow::Result<()> {
    let src_path = Path::new(runs_dir).join(run.subdir).join("buba-paint.db");

    log(&format!(
        "\nImporting run {} ({})...",
        run.subdir, run.version
    ));

    if !src_path.exists() {
        log(&format!("  SKIP -- {} not found", src_path.display()));
        return Ok(());
    }

    let run_id = run.number;

    conn.execute(
        "INSERT INTO runs (id, run_number, bot_version) VALUES (?1, ?2, ?3)",
        params![run_id, run_id, run.version],
    )?;

    let src_path_str = src_path
        .to_str()
        .context("source path is not valid UTF-8")?;
    let attach_sql = format!("ATTACH DATABASE 'file:{src_path_str}?mode=ro' AS src");
    conn.execute_batch(&attach_sql)
        .with_context(|| format!("attaching source DB: {src_path_str}"))?;

    log("  Copying tick_data...");
    conn.execute(
        &format!(
            "INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size, run_id)
             SELECT timestamp, source, price, bid, ask, bid_size, ask_size, {run_id}
             FROM src.tick_data"
        ),
        [],
    )?;

    let tick_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tick_data WHERE run_id = ?1",
        params![run_id],
        |r| r.get(0),
    )?;
    log(&format!("  tick_data: {tick_count} rows"));

    let has_pm_outcome = conn
        .prepare("SELECT polymarket_outcome FROM src.markets LIMIT 0")
        .is_ok();
    let has_outcome = conn
        .prepare("SELECT outcome FROM src.markets LIMIT 0")
        .is_ok();
    let has_resolution_source = conn
        .prepare("SELECT resolution_source FROM src.markets LIMIT 0")
        .is_ok();
    let has_fee_profile = conn
        .prepare("SELECT fee_profile FROM src.markets LIMIT 0")
        .is_ok();
    let has_order_min_size = conn
        .prepare("SELECT order_min_size FROM src.markets LIMIT 0")
        .is_ok();
    let has_tick_size = conn
        .prepare("SELECT order_price_min_tick_size FROM src.markets LIMIT 0")
        .is_ok();
    let supports_rebate_fee = conn
        .prepare("SELECT maker_base_fee FROM src.markets LIMIT 0")
        .is_ok();
    let supports_aggressor_fee = conn
        .prepare("SELECT taker_base_fee FROM src.markets LIMIT 0")
        .is_ok();
    let has_rewards_min_size = conn
        .prepare("SELECT rewards_min_size FROM src.markets LIMIT 0")
        .is_ok();
    let has_rewards_max_spread = conn
        .prepare("SELECT rewards_max_spread FROM src.markets LIMIT 0")
        .is_ok();

    log("  Copying markets...");
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO markets (
                market_id, question, condition_id, slug, up_token_id, down_token_id,
                start_time, end_time, status, outcome, polymarket_outcome, resolution_source,
                fee_profile, order_min_size, order_price_min_tick_size, maker_base_fee,
                taker_base_fee, rewards_min_size, rewards_max_spread, run_id
             )
             SELECT
                market_id, question, condition_id, slug, up_token_id, down_token_id,
                start_time, end_time, status,
                {outcome},
                {pm_outcome},
                {resolution_source},
                {fee_profile},
                {order_min_size},
                {tick_size},
                {maker_base_fee},
                {taker_base_fee},
                {rewards_min_size},
                {rewards_max_spread},
                {run_id}
             FROM src.markets",
            outcome = if has_outcome { "outcome" } else { "NULL" },
            pm_outcome = if has_pm_outcome {
                "polymarket_outcome"
            } else {
                "NULL"
            },
            resolution_source = if has_resolution_source {
                "resolution_source"
            } else {
                "NULL"
            },
            fee_profile = if has_fee_profile {
                "fee_profile"
            } else {
                "NULL"
            },
            order_min_size = if has_order_min_size {
                "order_min_size"
            } else {
                "NULL"
            },
            tick_size = if has_tick_size {
                "order_price_min_tick_size"
            } else {
                "NULL"
            },
            maker_base_fee = if supports_rebate_fee {
                "maker_base_fee"
            } else {
                "NULL"
            },
            taker_base_fee = if supports_aggressor_fee {
                "taker_base_fee"
            } else {
                "NULL"
            },
            rewards_min_size = if has_rewards_min_size {
                "rewards_min_size"
            } else {
                "NULL"
            },
            rewards_max_spread = if has_rewards_max_spread {
                "rewards_max_spread"
            } else {
                "NULL"
            },
        ),
        [],
    )?;

    let market_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE run_id = ?1",
        params![run_id],
        |r| r.get(0),
    )?;
    log(&format!("  markets: {market_count} rows"));

    let has_feed_events = conn
        .prepare("SELECT id FROM src.feed_events LIMIT 0")
        .is_ok();

    log("  Copying feed_events...");
    if has_feed_events {
        conn.execute(
            &format!(
                "INSERT INTO feed_events (
                    received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
                    price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity, run_id
                 )
                 SELECT
                    received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
                    price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity, {run_id}
                 FROM src.feed_events"
            ),
            [],
        )?;
    } else {
        conn.execute(
            &format!(
                "INSERT INTO feed_events (
                    received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
                    price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity, run_id
                 )
                 SELECT
                    timestamp, timestamp, source,
                    CASE source
                        WHEN 'binance' THEN 'binance_tick'
                        WHEN 'chainlink' THEN 'chainlink_price'
                        ELSE 'clob_snapshot'
                    END,
                    CASE
                        WHEN source IN ('clob_up', 'clob_down') THEN (
                            SELECT market_id
                            FROM src.markets m
                            WHERE m.start_time <= src.tick_data.timestamp
                              AND m.end_time >= src.tick_data.timestamp
                            ORDER BY m.start_time DESC
                            LIMIT 1
                        )
                        ELSE NULL
                    END,
                    CASE
                        WHEN source = 'clob_up' THEN (
                            SELECT up_token_id
                            FROM src.markets m
                            WHERE m.start_time <= src.tick_data.timestamp
                              AND m.end_time >= src.tick_data.timestamp
                            ORDER BY m.start_time DESC
                            LIMIT 1
                        )
                        WHEN source = 'clob_down' THEN (
                            SELECT down_token_id
                            FROM src.markets m
                            WHERE m.start_time <= src.tick_data.timestamp
                              AND m.end_time >= src.tick_data.timestamp
                            ORDER BY m.start_time DESC
                            LIMIT 1
                        )
                        ELSE NULL
                    END,
                    price,
                    bid,
                    ask,
                    bid_size,
                    ask_size,
                    NULL,
                    'legacy_snapshot',
                    {run_id}
                 FROM src.tick_data"
            ),
            [],
        )?;
    }

    let has_trade_pnl_net = conn
        .prepare("SELECT pnl_net FROM src.trade_results LIMIT 0")
        .is_ok();
    let has_trade_fee_amount = conn
        .prepare("SELECT fee_amount FROM src.trade_results LIMIT 0")
        .is_ok();
    let has_trade_fill_status = conn
        .prepare("SELECT fill_status FROM src.simulated_trades LIMIT 0")
        .is_ok();
    let has_trade_group_id = conn
        .prepare("SELECT execution_group_id FROM src.simulated_trades LIMIT 0")
        .is_ok();
    let has_trade_fidelity = conn
        .prepare("SELECT execution_fidelity FROM src.simulated_trades LIMIT 0")
        .is_ok();

    log("  Copying historical trades...");
    conn.execute(
        &format!(
            "INSERT INTO historical_trades (run_id, bot_version, timestamp, strategy, direction,
                entry_price, size, settlement_price, pnl, pnl_net, fee_amount, won,
                fill_status, execution_group_id, execution_fidelity)
             SELECT {run_id}, '{version}', st.timestamp, st.strategy, st.side,
                st.entry_price, st.size, tr.settlement_price, tr.pnl_0pct,
                {pnl_net_expr}, {fee_amount_expr},
                CASE WHEN {pnl_net_expr} > 0 THEN 1 ELSE 0 END,
                {fill_status_expr},
                {execution_group_expr},
                {execution_fidelity_expr}
             FROM src.simulated_trades st
             JOIN src.trade_results tr ON st.id = tr.trade_id
             WHERE st.status = 'closed'",
            version = run.version,
            pnl_net_expr = if has_trade_pnl_net {
                "COALESCE(tr.pnl_net, tr.pnl_0pct)"
            } else {
                "tr.pnl_0pct"
            },
            fee_amount_expr = if has_trade_fee_amount {
                "COALESCE(tr.fee_amount, 0)"
            } else {
                "0"
            },
            fill_status_expr = if has_trade_fill_status {
                "COALESCE(st.fill_status, 'legacy_assumed_full')"
            } else {
                "'legacy_assumed_full'"
            },
            execution_group_expr = if has_trade_group_id {
                "st.execution_group_id"
            } else {
                "NULL"
            },
            execution_fidelity_expr = if has_trade_fidelity {
                "COALESCE(st.execution_fidelity, 'legacy_snapshot')"
            } else {
                "'legacy_snapshot'"
            },
        ),
        [],
    )?;

    let trade_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM historical_trades WHERE run_id = ?1",
        params![run_id],
        |r| r.get(0),
    )?;
    log(&format!("  historical_trades: {trade_count} rows"));

    let time_range: (Option<i64>, Option<i64>) = conn.query_row(
        "SELECT MIN(timestamp), MAX(timestamp) FROM tick_data WHERE run_id = ?1",
        params![run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let (total_trades, wins): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(won), 0) FROM historical_trades WHERE run_id = ?1",
        params![run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let win_rate = if total_trades > 0 {
        wins as f64 / total_trades as f64
    } else {
        0.0
    };

    conn.execute(
        "UPDATE runs SET start_time = ?1, end_time = ?2, total_trades = ?3, win_rate = ?4
         WHERE run_number = ?5",
        params![time_range.0, time_range.1, total_trades, win_rate, run_id],
    )?;

    conn.execute_batch("DETACH src")?;

    Ok(())
}

/// Compute settlements.
fn compute_settlements(conn: &Connection) -> anyhow::Result<()> {
    log("Computing market settlements...");

    conn.execute_batch(
        "UPDATE markets SET open_price = (
            SELECT price FROM tick_data
            WHERE source = 'chainlink' AND timestamp <= markets.start_time
            ORDER BY timestamp DESC LIMIT 1
        )",
    )?;

    conn.execute_batch(
        "UPDATE markets SET close_price = (
            SELECT price FROM tick_data
            WHERE source = 'chainlink' AND timestamp <= markets.end_time
            ORDER BY timestamp DESC LIMIT 1
        )",
    )?;

    conn.execute_batch(
        "UPDATE markets SET outcome = CASE
            WHEN close_price >= open_price THEN 'UP'
            ELSE 'DOWN'
        END
        WHERE outcome IS NULL
          AND open_price IS NOT NULL
          AND close_price IS NOT NULL",
    )?;

    let settled: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE outcome IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))?;
    log(&format!(
        "  Settlements computed: {settled}/{total} markets"
    ));

    let missing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE outcome IS NULL",
        [],
        |r| r.get(0),
    )?;
    if missing > 0 {
        log(&format!(
            "  WARNING: {missing} markets without settlement (no chainlink data at boundary)"
        ));
    }

    let overridden: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE polymarket_outcome IS NOT NULL AND outcome != polymarket_outcome",
        [],
        |r| r.get(0),
    )?;
    conn.execute_batch(
        "UPDATE markets SET outcome = polymarket_outcome
         WHERE polymarket_outcome IS NOT NULL
           AND outcome != polymarket_outcome",
    )?;
    if overridden > 0 {
        log(&format!(
            "  Overrode {overridden} Chainlink outcomes with authoritative Polymarket outcomes"
        ));
    }
    let pm_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE polymarket_outcome IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    log(&format!(
        "  Polymarket outcomes available: {pm_count}/{total}"
    ));

    Ok(())
}

/// Compute data quality.
fn compute_data_quality(conn: &Connection) -> anyhow::Result<()> {
    log("Computing data quality metrics...");

    let range: (Option<i64>, Option<i64>) = conn.query_row(
        "SELECT MIN(timestamp), MAX(timestamp) FROM tick_data",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let (Some(tmin), Some(tmax)) = range else {
        log("  No tick data — skipping quality computation");
        return Ok(());
    };

    let start_hour = (tmin / 3_600_000) * 3_600_000;

    let end_hour = ((tmax + 3_600_000 - 1) / 3_600_000) * 3_600_000;

    let sources = ["binance", "chainlink", "clob_up", "clob_down"];

    let tx = conn.unchecked_transaction()?;
    let mut hour_count: u64 = 0;

    let mut hour = start_hour;
    while hour < end_hour {
        let hour_end = hour + 3_600_000;

        for source in &sources {
            let mut stmt = tx.prepare_cached(
                "SELECT timestamp FROM tick_data
                 WHERE source = ?1 AND timestamp >= ?2 AND timestamp < ?3
                 ORDER BY timestamp",
            )?;

            let timestamps: Vec<i64> = stmt
                .query_map(params![source, hour, hour_end], |r| r.get(0))?
                .collect::<Result<Vec<i64>, _>>()?;

            if timestamps.is_empty() {
                continue;
            }

            let mut gap_count_5s: i64 = 0;
            let mut gap_count_30s: i64 = 0;
            let mut max_gap: i64 = 0;

            for i in 1..timestamps.len() {
                let gap = timestamps[i] - timestamps[i - 1];
                if gap > 5_000 {
                    gap_count_5s += 1;
                }
                if gap > 30_000 {
                    gap_count_30s += 1;
                }
                if gap > max_gap {
                    max_gap = gap;
                }
            }

            #[allow(clippy::cast_possible_wrap)]
            let tick_count = timestamps.len() as i64;
            let coverage = tick_count as f64 / 3600.0;

            tx.execute(
                "INSERT INTO data_quality (hour_start, source, tick_count, expected_count,
                    gap_count_5s, gap_count_30s, max_gap_ms, coverage_pct)
                 VALUES (?1, ?2, ?3, 3600, ?4, ?5, ?6, ?7)",
                params![
                    hour,
                    source,
                    tick_count,
                    gap_count_5s,
                    gap_count_30s,
                    max_gap,
                    coverage
                ],
            )?;

            hour_count += 1;
        }

        hour += 3_600_000;
    }

    tx.commit()?;
    log(&format!(
        "  Computed quality for {hour_count} hour/source combinations"
    ));

    Ok(())
}

/// Print summary.
fn print_summary(conn: &Connection) -> anyhow::Result<()> {
    log("\n=== SUMMARY ===");

    let mut stmt = conn.prepare(
        "SELECT run_number, bot_version, start_time, end_time, total_trades, win_rate
         FROM runs ORDER BY run_number",
    )?;

    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, f64>(5)?,
        ))
    })?;

    for row in rows {
        let (run_number, version, start_time, end_time, total_trades, win_rate) = row?;

        let hours = match (start_time, end_time) {
            (Some(s), Some(e)) => format!("{:.1}h", (e - s) as f64 / 3_600_000.0),
            _ => "??h".to_string(),
        };

        let start_date = start_time.map_or_else(|| "??".to_string(), format_epoch_date);
        let end_date = end_time.map_or_else(|| "??".to_string(), format_epoch_date);

        log(&format!(
            "  Run {run_number:03}: {version} | {hours} | {total_trades} trades | \
             {wr:.1}% WR | {start_date} -> {end_date}",
            wr = win_rate * 100.0,
        ));
    }

    let tick_total: i64 = conn.query_row("SELECT COUNT(*) FROM tick_data", [], |r| r.get(0))?;
    let market_total: i64 = conn.query_row("SELECT COUNT(*) FROM markets", [], |r| r.get(0))?;
    let settled_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE outcome IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let trade_total: i64 =
        conn.query_row("SELECT COUNT(*) FROM historical_trades", [], |r| r.get(0))?;

    log(&format!("\n  Total ticks:   {tick_total}"));
    log(&format!(
        "  Total markets: {market_total} ({settled_total} with settlements)"
    ));
    log(&format!("  Total trades:  {trade_total}"));

    let low_quality: i64 = conn.query_row(
        "SELECT COUNT(*) FROM data_quality WHERE coverage_pct < 0.95",
        [],
        |r| r.get(0),
    )?;
    let total_quality: i64 =
        conn.query_row("SELECT COUNT(*) FROM data_quality", [], |r| r.get(0))?;
    log(&format!(
        "  Quality: {}/{total_quality} hours at >=95% coverage",
        total_quality - low_quality
    ));

    Ok(())
}

/// Prints one build-data progress line with the standard prefix.
fn log(msg: &str) {
    println!("[build] {msg}");
}

/// Format a millisecond epoch timestamp as YYYY-MM-DD.
fn format_epoch_date(epoch_ms: i64) -> String {
    let secs = epoch_ms / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0);
    dt.map_or_else(|| "??".to_string(), |d| d.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
#[path = "tests/build_data_tests.rs"]
mod tests;
