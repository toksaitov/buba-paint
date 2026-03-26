# Sweep rust-004, Rust parity validation

Date: 2026-03-14
Status: VALID, byte-for-byte identical to TS sweep 003 (excluding elapsed_s)

## Purpose

First Rust sweep. Validates that the Rust backtester produces numerically identical results to the TypeScript implementation. Same parameters, same data, same date range as sweep 003.

## Parameters

- Data: `data/market-data.db` (runs 004-007)
- Time range: 2026-02-20T03:13 to 2026-02-28T00:00 (188.8h)
- Ticks: 2,710,038
- Balance: $200
- Swept:
  - `LATENCY_ARB_MOMENTUM_THRESHOLD`: 0.001 to 0.003, step 0.0002 (11 values)
  - `LATENCY_ARB_MAX_ASK`: 0.45, 0.50, 0.55, 0.60, 0.65 (5 values)
  - `MAX_POSITION_FRACTION`: 0.05, 0.075, 0.10, 0.125, 0.15 (5 values)
- Fixed:
  - `SPREAD_CAPTURE_THRESHOLD=0.50` (disables spread-capture)
  - `PEAK_DD_PAUSE_PCT=1.0` (disables peak drawdown pause)
- Total: 275 combinations
- Runtime: ~42s (vs ~40 min for TS sweep 003, 57x faster)

## Command

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-004/sweep.csv
```

## Top 5 by PnL

1. mom=0.0012, ask=0.65, frac=0.125: $4,070, 60.9% WR, 284 trades, 46.4% DD
2. mom=0.0012, ask=0.60, frac=0.15: $3,490, 59.2% WR, 238 trades, 44.4% DD
3. mom=0.0012, ask=0.65, frac=0.10: $3,386, 60.9% WR, 284 trades, 40.3% DD
4. mom=0.0012, ask=0.60, frac=0.125: $3,360, 59.2% WR, 238 trades, 42.8% DD
5. mom=0.0012, ask=0.60, frac=0.10: $2,893, 59.2% WR, 238 trades, 39.0% DD

## Key finding

Byte-for-byte parity with TS sweep 003 on all columns except elapsed_s. Confirms the Rust port is numerically identical. The 57x speedup (42s vs 40 min) comes from rayon parallelism across all CPU cores.
