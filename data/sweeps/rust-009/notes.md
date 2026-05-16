# Sweep rust-009, v0.8 backward compatibility verification

Date: 2026-03-29
Status: VALID, confirms v0.8 code produces identical backtester results to v0.7

## Purpose

Re-ran the rust-008 parameter grid with v0.8 code (provisional settlement, Polymarket SDK integration, executor abstraction). The backtester code path is unchanged between v0.7 and v0.8 (it uses synchronous Chainlink-based settlement, not the live provisional system). This sweep verifies that the new code did not accidentally alter backtesting behavior.

## Parameters

Identical to rust-008.

* Data: `data/market-data.db` (runs 004-008, rebuilt from scratch)
* Time range: 2026-02-15 to 2026-03-27 (~928h, ~39 days)
* Ticks: 11,074,264
* Balance: $200
* Swept: 6 x 5 x 5 = 150 combinations (same grid as rust-008)
* Fixed: `SPREAD_CAPTURE_THRESHOLD=0.50`, `PEAK_DD_PAUSE_PCT=1.0`
* Runtime: 175s (~3 min)

## Command

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-15 --end 2026-03-27 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-009/sweep.csv
```

## Key finding

All 150 rows match rust-008 byte-for-byte on every column except elapsed_s. The v0.8 changes (provisional settlement, SDK, executor trait, new DB columns) do not affect the backtester path.
