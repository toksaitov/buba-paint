// Build merged market-data DB from individual run databases.
//
// Rust port of `legacy-ts/src/data/build-market-db.ts`.
// Merges tick_data, markets, and trade results from runs/004-007 into a
// single `data/market-data.db`.  Source DBs are never modified (attached
// read-only).

use std::path::Path;

use anyhow::Context;
use rusqlite::{Connection, params};

// ---------------------------------------------------------------------------
// Run metadata
// ---------------------------------------------------------------------------

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
];

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

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
        run_id        INTEGER NOT NULL,
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
        won              INTEGER NOT NULL,
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
    CREATE INDEX IF NOT EXISTS idx_htrades_run ON historical_trades(run_id);
";

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Build a merged market-data database from individual run databases.
///
/// Source DBs are opened read-only via `ATTACH DATABASE ... ?mode=ro`.
/// The output file is deleted and rebuilt from scratch each time.
pub fn build_market_data(runs_dir: &str, output: &str) -> anyhow::Result<()> {
    log("Building merged market data DB...");
    log(&format!("Output: {output}"));

    // 1. Delete output file if it exists.
    if Path::new(output).exists() {
        std::fs::remove_file(output).context("removing existing output DB")?;
        log("Removed existing output DB");
    }

    // Ensure parent directory exists.
    if let Some(parent) = Path::new(output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory for output: {}", parent.display()))?;
        }
    }

    // 2. Create new SQLite DB.
    let conn = Connection::open(output).context("creating output database")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "cache_size", "-256000")?;

    // 3. Create schema.
    conn.execute_batch(CREATE_SCHEMA)
        .context("creating schema")?;

    // 4. Import each run.
    for run in RUNS {
        import_run(&conn, runs_dir, run)?;
    }

    // 5. Create indexes (before settlement computation — needs source+timestamp index).
    log("Creating indexes...");
    conn.execute_batch(CREATE_INDEXES)
        .context("creating indexes")?;

    // 6. Compute settlements.
    compute_settlements(&conn)?;

    // 7. Compute data quality metrics.
    compute_data_quality(&conn)?;

    // 8. Print summary.
    print_summary(&conn)?;

    // 9. VACUUM.
    log("\nVACUUMing...");
    conn.execute_batch("VACUUM")?;

    log("\nDone!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Import a single run
// ---------------------------------------------------------------------------

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

    // Insert run provenance (explicit id = run number for FK consistency).
    conn.execute(
        "INSERT INTO runs (id, run_number, bot_version) VALUES (?1, ?2, ?3)",
        params![run_id, run_id, run.version],
    )?;

    // ATTACH source DB as read-only.
    let src_path_str = src_path
        .to_str()
        .context("source path is not valid UTF-8")?;
    let attach_sql = format!("ATTACH DATABASE 'file:{src_path_str}?mode=ro' AS src");
    conn.execute_batch(&attach_sql)
        .with_context(|| format!("attaching source DB: {src_path_str}"))?;

    // Copy tick_data.
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

    // Copy markets (INSERT OR IGNORE to handle duplicates across runs).
    log("  Copying markets...");
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO markets (market_id, question, condition_id, slug,
                up_token_id, down_token_id, start_time, end_time, status, run_id)
             SELECT market_id, question, condition_id, slug,
                up_token_id, down_token_id, start_time, end_time, status, {run_id}
             FROM src.markets"
        ),
        [],
    )?;

    let market_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM markets WHERE run_id = ?1",
        params![run_id],
        |r| r.get(0),
    )?;
    log(&format!("  markets: {market_count} rows"));

    // Copy historical trades (join simulated_trades + trade_results).
    log("  Copying historical trades...");
    conn.execute(
        &format!(
            "INSERT INTO historical_trades (run_id, bot_version, timestamp, strategy, direction,
                entry_price, size, settlement_price, pnl, won)
             SELECT {run_id}, '{version}', st.timestamp, st.strategy, st.side,
                st.entry_price, st.size, tr.settlement_price, tr.pnl_0pct,
                CASE WHEN tr.pnl_0pct > 0 THEN 1 ELSE 0 END
             FROM src.simulated_trades st
             JOIN src.trade_results tr ON st.id = tr.trade_id
             WHERE st.status = 'closed'",
            version = run.version
        ),
        [],
    )?;

    let trade_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM historical_trades WHERE run_id = ?1",
        params![run_id],
        |r| r.get(0),
    )?;
    log(&format!("  historical_trades: {trade_count} rows"));

    // Update run time range and stats.
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

    // Detach source DB.
    conn.execute_batch("DETACH src")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Settlement computation
// ---------------------------------------------------------------------------

fn compute_settlements(conn: &Connection) -> anyhow::Result<()> {
    log("Computing market settlements...");

    // Open price: latest chainlink tick at or before window start.
    conn.execute_batch(
        "UPDATE markets SET open_price = (
            SELECT price FROM tick_data
            WHERE source = 'chainlink' AND timestamp <= markets.start_time
            ORDER BY timestamp DESC LIMIT 1
        )",
    )?;

    // Close price: latest chainlink tick at or before window end.
    conn.execute_batch(
        "UPDATE markets SET close_price = (
            SELECT price FROM tick_data
            WHERE source = 'chainlink' AND timestamp <= markets.end_time
            ORDER BY timestamp DESC LIMIT 1
        )",
    )?;

    // Outcome: UP if close >= open, DOWN otherwise.
    conn.execute_batch(
        "UPDATE markets SET outcome = CASE
            WHEN close_price >= open_price THEN 'UP'
            ELSE 'DOWN'
        END
        WHERE open_price IS NOT NULL AND close_price IS NOT NULL",
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

    Ok(())
}

// ---------------------------------------------------------------------------
// Data quality computation
// ---------------------------------------------------------------------------

fn compute_data_quality(conn: &Connection) -> anyhow::Result<()> {
    log("Computing data quality metrics...");

    // Get time range.
    let range: (Option<i64>, Option<i64>) = conn.query_row(
        "SELECT MIN(timestamp), MAX(timestamp) FROM tick_data",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let (Some(tmin), Some(tmax)) = range else {
        log("  No tick data — skipping quality computation");
        return Ok(());
    };

    // Round to hour boundaries.
    let start_hour = (tmin / 3_600_000) * 3_600_000;
    // Ceiling division for end hour.
    let end_hour = ((tmax + 3_600_000 - 1) / 3_600_000) * 3_600_000;

    let sources = ["binance", "chainlink", "clob_up", "clob_down"];

    let tx = conn.unchecked_transaction()?;
    let mut hour_count: u64 = 0;

    let mut hour = start_hour;
    while hour < end_hour {
        let hour_end = hour + 3_600_000;

        for source in &sources {
            // Fetch all tick timestamps in this hour for this source.
            let mut stmt = tx.prepare_cached(
                "SELECT timestamp FROM tick_data
                 WHERE source = ?1 AND timestamp >= ?2 AND timestamp < ?3
                 ORDER BY timestamp",
            )?;

            let timestamps: Vec<i64> = stmt
                .query_map(params![source, hour, hour_end], |r| r.get(0))?
                .collect::<Result<Vec<i64>, _>>()?;

            if timestamps.is_empty() {
                continue; // No data in this hour (gap between runs).
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

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

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

    // Data quality overview.
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn log(msg: &str) {
    println!("[build] {msg}");
}

/// Format a millisecond epoch timestamp as YYYY-MM-DD.
fn format_epoch_date(epoch_ms: i64) -> String {
    let secs = epoch_ms / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0);
    dt.map_or_else(|| "??".to_string(), |d| d.format("%Y-%m-%d").to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/build_data_tests.rs"]
mod tests;
