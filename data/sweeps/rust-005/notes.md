# Sweep rust-005, post-refactor parity check

Date: 2026-03-20
Status: VALID, byte-for-byte identical to rust-004 (excluding elapsed_s)

## Purpose

Re-ran the rust-004 sweep after significant code refactoring (workspace restructure, agent and dashboard additions). Confirms refactoring did not alter backtesting behavior.

## Parameters

Identical to rust-004. Same data, same grid, same fixed params.

* Data: `data/market-data.db` (runs 004-007)
* Time range: 2026-02-20T03:13 to 2026-02-28T00:00 (188.8h)
* Ticks: 2,710,038
* Balance: $200
* Swept: 11 x 5 x 5 = 275 combinations (same grid as rust-004)
* Fixed: `SPREAD_CAPTURE_THRESHOLD=0.50`, `PEAK_DD_PAUSE_PCT=1.0`
* Runtime: ~42s

## Command

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-005/sweep.csv
```

## Key finding

All 275 rows match rust-004 exactly (excluding elapsed_s). The workspace restructure, agent addition, and dashboard addition did not alter any backtesting logic.
