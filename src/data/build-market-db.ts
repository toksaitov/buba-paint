/**
 * Builds the merged market-data.db from individual run databases.
 * Usage: npx tsx src/data/build-market-db.ts (or: npm run build-data)
 *
 * This script is idempotent — it deletes and rebuilds the output DB from scratch.
 * Original run DBs are never modified.
 */

import BetterSqlite3 from "better-sqlite3";
import { existsSync, unlinkSync } from "node:fs";
import { resolve, dirname } from "node:path";

const PROJECT_ROOT = resolve(dirname(new URL(import.meta.url).pathname), "../..");

const RUNS: Array<{ number: number; version: string; path: string }> = [
  { number: 4, version: "v0.2", path: "runs/004/buba-paint.db" },
  { number: 5, version: "v0.3", path: "runs/005/buba-paint.db" },
  { number: 6, version: "v0.4", path: "runs/006/buba-paint.db" },
  { number: 7, version: "v0.5", path: "runs/007/buba-paint.db" },
];

const OUTPUT = resolve(PROJECT_ROOT, "data/market-data.db");

function log(msg: string): void {
  console.log(`[build] ${msg}`);
}

function createSchema(db: BetterSqlite3.Database): void {
  db.exec(`
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
  `);
}

function importRun(merged: BetterSqlite3.Database, run: (typeof RUNS)[0]): void {
  const srcPath = resolve(PROJECT_ROOT, run.path);
  if (!existsSync(srcPath)) {
    log(`  SKIP — ${srcPath} not found`);
    return;
  }

  // Insert run provenance (explicit id = run number for FK consistency)
  merged.prepare(
    `INSERT INTO runs (id, run_number, bot_version) VALUES (?, ?, ?)`,
  ).run(run.number, run.number, run.version);
  const runId = run.number;

  // Attach source DB
  merged.exec(`ATTACH DATABASE '${srcPath}' AS src`);

  // Copy tick_data (bulk — let SQLite handle it internally)
  log(`  Copying tick_data...`);
  const tickResult = merged.exec(`
    INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size, run_id)
    SELECT timestamp, source, price, bid, ask, bid_size, ask_size, ${runId}
    FROM src.tick_data
  `);

  const tickCount = merged.prepare(
    `SELECT COUNT(*) as n FROM tick_data WHERE run_id = ?`,
  ).get(runId) as { n: number };
  log(`  tick_data: ${tickCount.n.toLocaleString()} rows`);

  // Copy markets
  log(`  Copying markets...`);
  merged.exec(`
    INSERT OR IGNORE INTO markets (market_id, question, condition_id, slug,
      up_token_id, down_token_id, start_time, end_time, status, run_id)
    SELECT market_id, question, condition_id, slug,
      up_token_id, down_token_id, start_time, end_time, status, ${runId}
    FROM src.markets
  `);

  const marketCount = merged.prepare(
    `SELECT COUNT(*) as n FROM markets WHERE run_id = ?`,
  ).get(runId) as { n: number };
  log(`  markets: ${marketCount.n.toLocaleString()} rows`);

  // Copy historical trades (join simulated_trades + trade_results)
  log(`  Copying historical trades...`);
  merged.exec(`
    INSERT INTO historical_trades (run_id, bot_version, timestamp, strategy, direction,
      entry_price, size, settlement_price, pnl, won)
    SELECT ${runId}, '${run.version}', st.timestamp, st.strategy, st.side,
      st.entry_price, st.size, tr.settlement_price, tr.pnl_0pct,
      CASE WHEN tr.pnl_0pct > 0 THEN 1 ELSE 0 END
    FROM src.simulated_trades st
    JOIN src.trade_results tr ON st.id = tr.trade_id
    WHERE st.status = 'closed'
  `);

  const tradeCount = merged.prepare(
    `SELECT COUNT(*) as n FROM historical_trades WHERE run_id = ?`,
  ).get(runId) as { n: number };
  log(`  historical_trades: ${tradeCount.n.toLocaleString()} rows`);

  // Update run time range and stats
  const timeRange = merged.prepare(`
    SELECT MIN(timestamp) as start_time, MAX(timestamp) as end_time
    FROM tick_data WHERE run_id = ?
  `).get(runId) as { start_time: number; end_time: number };

  const stats = merged.prepare(`
    SELECT COUNT(*) as total, SUM(won) as wins
    FROM historical_trades WHERE run_id = ?
  `).get(runId) as { total: number; wins: number };

  merged.prepare(`
    UPDATE runs SET start_time = ?, end_time = ?, total_trades = ?, win_rate = ?
    WHERE run_number = ?
  `).run(
    timeRange.start_time,
    timeRange.end_time,
    stats.total,
    stats.total > 0 ? stats.wins / stats.total : 0,
    run.number,
  );

  merged.exec(`DETACH src`);
}

function computeSettlements(db: BetterSqlite3.Database): void {
  log("Computing market settlements...");

  // Open price: latest chainlink tick at or before window start
  db.exec(`
    UPDATE markets SET open_price = (
      SELECT price FROM tick_data
      WHERE source = 'chainlink' AND timestamp <= markets.start_time
      ORDER BY timestamp DESC LIMIT 1
    )
  `);

  // Close price: latest chainlink tick at or before window end
  db.exec(`
    UPDATE markets SET close_price = (
      SELECT price FROM tick_data
      WHERE source = 'chainlink' AND timestamp <= markets.end_time
      ORDER BY timestamp DESC LIMIT 1
    )
  `);

  // Outcome: UP if close >= open, DOWN otherwise
  db.exec(`
    UPDATE markets SET outcome = CASE
      WHEN close_price >= open_price THEN 'UP'
      ELSE 'DOWN'
    END
    WHERE open_price IS NOT NULL AND close_price IS NOT NULL
  `);

  const settled = db.prepare(
    `SELECT COUNT(*) as n FROM markets WHERE outcome IS NOT NULL`,
  ).get() as { n: number };
  const total = db.prepare(
    `SELECT COUNT(*) as n FROM markets`,
  ).get() as { n: number };
  log(`  Settlements computed: ${settled.n}/${total.n} markets`);

  const missing = db.prepare(
    `SELECT COUNT(*) as n FROM markets WHERE outcome IS NULL`,
  ).get() as { n: number };
  if (missing.n > 0) {
    log(`  WARNING: ${missing.n} markets without settlement (no chainlink data at boundary)`);
  }
}

function computeDataQuality(db: BetterSqlite3.Database): void {
  log("Computing data quality metrics...");

  // Get time range
  const range = db.prepare(
    `SELECT MIN(timestamp) as tmin, MAX(timestamp) as tmax FROM tick_data`,
  ).get() as { tmin: number; tmax: number };

  // Round to hour boundaries
  const startHour = Math.floor(range.tmin / 3_600_000) * 3_600_000;
  const endHour = Math.ceil(range.tmax / 3_600_000) * 3_600_000;

  const sources = ["binance", "chainlink", "clob_up", "clob_down"];

  const insertQuality = db.prepare(`
    INSERT INTO data_quality (hour_start, source, tick_count, expected_count,
      gap_count_5s, gap_count_30s, max_gap_ms, coverage_pct)
    VALUES (?, ?, ?, 3600, ?, ?, ?, ?)
  `);

  // For each hour and source, compute metrics
  // We'll do this efficiently with a single pass approach
  const getTicksInHour = db.prepare(`
    SELECT timestamp FROM tick_data
    WHERE source = ? AND timestamp >= ? AND timestamp < ?
    ORDER BY timestamp
  `);

  let hourCount = 0;
  const totalHours = Math.ceil((endHour - startHour) / 3_600_000) * sources.length;

  const insertMany = db.transaction(() => {
    for (let hour = startHour; hour < endHour; hour += 3_600_000) {
      for (const source of sources) {
        const ticks = getTicksInHour.all(source, hour, hour + 3_600_000) as Array<{
          timestamp: number;
        }>;

        if (ticks.length === 0) continue; // No data in this hour (gap between runs)

        let gapCount5s = 0;
        let gapCount30s = 0;
        let maxGap = 0;

        for (let i = 1; i < ticks.length; i++) {
          const gap = ticks[i].timestamp - ticks[i - 1].timestamp;
          if (gap > 5_000) gapCount5s++;
          if (gap > 30_000) gapCount30s++;
          if (gap > maxGap) maxGap = gap;
        }

        const coverage = ticks.length / 3600;

        insertQuality.run(hour, source, ticks.length, gapCount5s, gapCount30s, maxGap, coverage);
        hourCount++;
      }
    }
  });

  insertMany();
  log(`  Computed quality for ${hourCount} hour/source combinations`);
}

function createIndexes(db: BetterSqlite3.Database): void {
  log("Creating indexes...");
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_tick_ts ON tick_data(timestamp);
    CREATE INDEX IF NOT EXISTS idx_tick_source_ts ON tick_data(source, timestamp);
    CREATE INDEX IF NOT EXISTS idx_tick_run ON tick_data(run_id);
    CREATE INDEX IF NOT EXISTS idx_markets_start ON markets(start_time);
    CREATE INDEX IF NOT EXISTS idx_markets_end ON markets(end_time);
    CREATE INDEX IF NOT EXISTS idx_markets_outcome ON markets(outcome);
    CREATE INDEX IF NOT EXISTS idx_markets_run ON markets(run_id);
    CREATE INDEX IF NOT EXISTS idx_htrades_run ON historical_trades(run_id);
  `);
}

function printSummary(db: BetterSqlite3.Database): void {
  log("\n=== SUMMARY ===");

  const runs = db.prepare(`SELECT * FROM runs ORDER BY run_number`).all() as Array<{
    run_number: number;
    bot_version: string;
    start_time: number;
    end_time: number;
    total_trades: number;
    win_rate: number;
  }>;

  for (const r of runs) {
    const hours = ((r.end_time - r.start_time) / 3_600_000).toFixed(1);
    log(
      `  Run ${String(r.run_number).padStart(3, "0")}: ${r.bot_version} | ` +
      `${hours}h | ${r.total_trades} trades | ` +
      `${(r.win_rate * 100).toFixed(1)}% WR | ` +
      `${new Date(r.start_time).toISOString().slice(0, 10)} → ${new Date(r.end_time).toISOString().slice(0, 10)}`,
    );
  }

  const tickTotal = db.prepare(`SELECT COUNT(*) as n FROM tick_data`).get() as { n: number };
  const marketTotal = db.prepare(`SELECT COUNT(*) as n FROM markets`).get() as { n: number };
  const settledTotal = db.prepare(
    `SELECT COUNT(*) as n FROM markets WHERE outcome IS NOT NULL`,
  ).get() as { n: number };
  const tradeTotal = db.prepare(`SELECT COUNT(*) as n FROM historical_trades`).get() as { n: number };

  log(`\n  Total ticks:   ${tickTotal.n.toLocaleString()}`);
  log(`  Total markets: ${marketTotal.n.toLocaleString()} (${settledTotal.n} with settlements)`);
  log(`  Total trades:  ${tradeTotal.n.toLocaleString()}`);

  // Data quality overview
  const lowQuality = db.prepare(
    `SELECT COUNT(*) as n FROM data_quality WHERE coverage_pct < 0.95`,
  ).get() as { n: number };
  const totalQuality = db.prepare(
    `SELECT COUNT(*) as n FROM data_quality`,
  ).get() as { n: number };
  log(`  Quality: ${totalQuality.n - lowQuality.n}/${totalQuality.n} hours at >=95% coverage`);
}

// === Main ===

function main(): void {
  log("Building merged market data DB...");
  log(`Output: ${OUTPUT}`);

  // Remove existing output
  if (existsSync(OUTPUT)) {
    unlinkSync(OUTPUT);
    log("Removed existing output DB");
  }

  const db = new BetterSqlite3(OUTPUT);
  db.pragma("journal_mode = WAL");
  db.pragma("synchronous = OFF"); // Speed — we're building, not production
  db.pragma("cache_size = -256000"); // 256MB cache for bulk ops

  createSchema(db);

  // Import each run
  for (const run of RUNS) {
    log(`\nImporting run ${String(run.number).padStart(3, "0")} (${run.version})...`);
    importRun(db, run);
  }

  // Create indexes before settlement computation (needs source+timestamp index)
  createIndexes(db);

  // Compute settlements
  computeSettlements(db);

  // Compute data quality
  computeDataQuality(db);

  // Print summary
  printSummary(db);

  // Compact
  log("\nVACUUMing...");
  db.exec("VACUUM");

  db.close();
  log("\nDone!");
}

main();
