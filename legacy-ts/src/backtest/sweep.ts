#!/usr/bin/env npx tsx
export {};
/**
 * Parameter sweep — runs the backtester with many parameter combinations.
 *
 * Usage:
 *   npx tsx src/backtest/sweep.ts \
 *     --data data/market-data.db \
 *     --start 2026-02-20 --end 2026-03-04 \
 *     --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0005 \
 *     --sweep LATENCY_ARB_MAX_ASK=0.45:0.60:0.05 \
 *     --sweep MAX_POSITION_FRACTION=0.05:0.15:0.025 \
 *     --output data/sweeps/001/sweep.csv
 *
 * Sweep format: PARAM=start:end:step
 */

import { writeFileSync, mkdirSync, unlinkSync, existsSync } from "node:fs";
import { dirname } from "node:path";

const args = process.argv.slice(2);

function getArg(name: string, fallback?: string): string {
  const idx = args.indexOf(`--${name}`);
  if (idx === -1 || idx + 1 >= args.length) {
    if (fallback !== undefined) return fallback;
    console.error(`Missing required argument: --${name}`);
    process.exit(1);
  }
  return args[idx + 1];
}

function getAllArgs(name: string): string[] {
  const values: string[] = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === `--${name}` && i + 1 < args.length) {
      values.push(args[i + 1]);
    }
  }
  return values;
}

const dataPath = getArg("data", "data/market-data.db");
const startStr = getArg("start");
const endStr = getArg("end");
const outputPath = getArg("output", "data/sweeps/001/sweep.csv");
const startingBalance = parseFloat(getArg("balance", "200"));

const fixedOverrides = getAllArgs("set").map((s) => {
  const eqIdx = s.indexOf("=");
  if (eqIdx === -1) { console.error(`Invalid --set: ${s}`); process.exit(1); }
  return { param: s.slice(0, eqIdx), value: s.slice(eqIdx + 1) };
});

const toUtc = (s: string) => /[Z+-]/.test(s.slice(-6)) ? s : s + "Z";
const startTime = new Date(toUtc(startStr)).getTime();
const endTime = new Date(toUtc(endStr)).getTime();

if (isNaN(startTime) || isNaN(endTime)) {
  console.error(`Invalid date range: ${startStr} → ${endStr}`);
  process.exit(1);
}

interface SweepDimension {
  param: string;
  values: number[];
}

const sweepSpecs = getAllArgs("sweep");
const dimensions: SweepDimension[] = [];

for (const spec of sweepSpecs) {
  const eqIdx = spec.indexOf("=");
  if (eqIdx === -1) {
    console.error(`Invalid sweep format: ${spec} (expected PARAM=start:end:step)`);
    process.exit(1);
  }
  const param = spec.slice(0, eqIdx);
  const range = spec.slice(eqIdx + 1);

  let values: number[];
  if (range.includes(",")) {
    values = range.split(",").map(Number);
  } else {
    const parts = range.split(":");
    if (parts.length !== 3) {
      console.error(`Invalid range: ${range} (expected start:end:step)`);
      process.exit(1);
    }
    const [start, end, step] = parts.map(Number);
    values = [];
    for (let v = start; v <= end + step * 0.001; v += step) {
      values.push(parseFloat(v.toPrecision(10)));
    }
  }

  dimensions.push({ param, values });
}

function* cartesian(dims: SweepDimension[]): Generator<Array<{ param: string; value: number }>> {
  if (dims.length === 0) {
    yield [];
    return;
  }
  const [first, ...rest] = dims;
  for (const value of first.values) {
    for (const combo of cartesian(rest)) {
      yield [{ param: first.param, value }, ...combo];
    }
  }
}

const combinations = [...cartesian(dimensions)];
const totalRuns = combinations.length;

console.log(`\nSweep: ${dimensions.map((d) => `${d.param}(${d.values.length})`).join(" × ")} = ${totalRuns} combinations\n`);

const { runBacktest } = await import("./runner.js");
const { TickReplay } = await import("./tick-replay.js");
const BetterSqlite3 = (await import("better-sqlite3")).default;

console.log("Loading tick data into memory...");
const cacheDb = new BetterSqlite3(dataPath, { readonly: true });
const cachedTicks = TickReplay.loadTicks(cacheDb, startTime, endTime);
cacheDb.close();
console.log(`Loaded ${cachedTicks.length.toLocaleString()} ticks.\n`);

interface SweepRow {
  [key: string]: string | number;
}

const results: SweepRow[] = [];
const t0 = Date.now();

for (let i = 0; i < combinations.length; i++) {
  const combo = combinations[i];

  for (const { param, value } of combo) {
    process.env[param] = String(value);
  }
  process.env.STARTING_BALANCE = String(startingBalance);
  process.env.LOG_LEVEL = "error";

  const { CONFIG } = await import("../config.js");
  for (const { param, value } of combo) {
    (CONFIG as Record<string, unknown>)[param] = value;
  }
  for (const { param, value } of fixedOverrides) {
    const num = parseFloat(value);
    (CONFIG as Record<string, unknown>)[param] = isNaN(num) ? value : num;
  }
  (CONFIG as Record<string, unknown>).STARTING_BALANCE = startingBalance;
  (CONFIG as Record<string, unknown>).LOG_LEVEL = "error";

  const resultsDbPath = `/tmp/buba-sweep-${String(i).padStart(4, "0")}.db`;

  const label = combo.map(({ param, value }) => `${param}=${value}`).join(" ");
  process.stdout.write(`[${i + 1}/${totalRuns}] ${label} ... `);

  const result = runBacktest({
    dataDbPath: dataPath,
    resultsDbPath,
    startTime,
    endTime,
    startingBalance,
    quiet: true,
    cachedTicks,
  });

  console.log(
    `PnL=$${result.totalPnl.toFixed(0)} WR=${(result.winRate * 100).toFixed(1)}% ` +
    `Trades=${result.trades} DD=${(result.maxDrawdownPct * 100).toFixed(1)}% ` +
    `(${result.elapsedSeconds.toFixed(1)}s)`,
  );

  const row: SweepRow = {};
  for (const { param, value } of combo) {
    row[param] = value;
  }
  row.pnl = result.totalPnl;
  row.win_rate = result.winRate;
  row.trades = result.trades;
  row.wins = result.wins;
  row.losses = result.losses;
  row.max_dd = result.maxDrawdownPct;
  row.hwm = result.highWaterMark;
  row.final_balance = result.finalBalance;
  row.signals = result.signals;
  row.elapsed_s = result.elapsedSeconds;
  results.push(row);

  for (const suffix of ["", "-shm", "-wal"]) {
    const f = resultsDbPath + suffix;
    if (existsSync(f)) try { unlinkSync(f); } catch {}
  }
}

mkdirSync(dirname(outputPath), { recursive: true });
const headers = Object.keys(results[0]);
const csv = [
  headers.join(","),
  ...results.map((row) => headers.map((h) => row[h]).join(",")),
].join("\n");
writeFileSync(outputPath, csv + "\n");

const totalElapsed = ((Date.now() - t0) / 1000).toFixed(1);
console.log(`\nSweep complete: ${totalRuns} runs in ${totalElapsed}s`);
console.log(`Results: ${outputPath}`);

// Print top 5 by PnL
const sorted = [...results].sort((a, b) => (b.pnl as number) - (a.pnl as number));
console.log("\nTop 5 by PnL:");
const paramNames = dimensions.map((d) => d.param);
for (let i = 0; i < Math.min(5, sorted.length); i++) {
  const r = sorted[i];
  const params = paramNames.map((p) => `${p}=${r[p]}`).join(" ");
  console.log(
    `  ${i + 1}. ${params} → PnL=$${(r.pnl as number).toFixed(0)} ` +
    `WR=${((r.win_rate as number) * 100).toFixed(1)}% DD=${((r.max_dd as number) * 100).toFixed(1)}%`,
  );
}
