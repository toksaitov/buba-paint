#!/usr/bin/env npx tsx
export {};
/**
 * Backtester CLI entry point.
 *
 * Usage:
 *   npx tsx src/backtest/run.ts --data data/market-data.db \
 *     --start 2026-02-20 --end 2026-03-04 \
 *     --results backtest/results/test.db \
 *     --set LATENCY_ARB_MAX_ASK=0.55 \
 *     --set MAX_POSITION_FRACTION=0.10
 *
 * Config overrides are set as env vars BEFORE importing CONFIG,
 * so the same config.ts code picks them up naturally.
 */

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
const resultsPath = getArg("results", "backtest/results/backtest.db");
const startStr = getArg("start");
const endStr = getArg("end");
const startingBalance = parseFloat(getArg("balance", "200"));

const overrides = getAllArgs("set");
for (const override of overrides) {
  const eqIdx = override.indexOf("=");
  if (eqIdx === -1) {
    console.error(`Invalid --set format: ${override} (expected KEY=VALUE)`);
    process.exit(1);
  }
  const key = override.slice(0, eqIdx);
  const value = override.slice(eqIdx + 1);
  process.env[key] = value;
}

process.env.STARTING_BALANCE = String(startingBalance);

process.env.LOG_LEVEL = process.env.LOG_LEVEL ?? "warn";

const { runBacktest } = await import("./runner.js");
const toUtc = (s: string) => /[Z+-]/.test(s.slice(-6)) ? s : s + "Z";
const startTime = new Date(toUtc(startStr)).getTime();
const endTime = new Date(toUtc(endStr)).getTime();

if (isNaN(startTime) || isNaN(endTime)) {
  console.error(`Invalid date range: ${startStr} → ${endStr}`);
  process.exit(1);
}

runBacktest({
  dataDbPath: dataPath,
  resultsDbPath: resultsPath,
  startTime,
  endTime,
  startingBalance,
});
