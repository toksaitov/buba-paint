# Sweep 002, valid but throttled by v0.5 peak DD pause

Date: 2026-03-13
Status: valid results, but peak DD pause (v0.5 feature) severely limits performance at $200 starting balance

## Parameters

Identical to sweep 001 (post-refactoring validation run).

- Data: `data/market-data.db` (runs 004-007)
- Time range: 2026-02-20T03:13 -> 2026-02-28T00:00 (188.8h)
- Balance: $200
- Swept:
  - `LATENCY_ARB_MOMENTUM_THRESHOLD`: 0.001 -> 0.003, step 0.0002 (11 values)
  - `LATENCY_ARB_MAX_ASK`: 0.45, 0.50, 0.55, 0.60, 0.65 (5 values)
  - `MAX_POSITION_FRACTION`: 0.05, 0.075, 0.10, 0.125, 0.15 (5 values)
- Fixed: `SPREAD_CAPTURE_THRESHOLD=0.50` (disables spread-capture)
- Total: 275 combinations
- Runtime: ~39 min (2,368s)

## Command

```bash
npm run sweep -- \
  --data data/market-data.db \
  --output data/sweeps/002/sweep.csv \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50
```

## Top 5 by PnL

| mom | ask | frac | PnL | WR | Trades | MaxDD |
|---|---|---|---|---|---|---|
| 0.0012 | 0.60 | 0.075 | $1,282 | 58.2% | 201 | 31.8% |
| 0.0012 | 0.65 | 0.05 | $1,087 | 60.9% | 284 | 24.8% |
| 0.0012 | 0.60 | 0.05 | $1,086 | 59.2% | 238 | 22.1% |
| 0.0022 | 0.65 | 0.10 | $1,002 | 73.2% | 56 | 26.3% |
| 0.0014 | 0.65 | 0.05 | $737 | 60.8% | 199 | 22.7% |

## Key finding: peak DD pause throttles small-balance backtests

The v0.5 `PEAK_DD_PAUSE_PCT=0.30` triggers after just ~$60 loss from $200 start. This pauses trading for 1 hour (`PEAK_DD_PAUSE_MS=3,600,000`), missing profitable windows. Live run 006 (v0.4) had no peak DD pause and made $4,765.

Verification: a single backtest with identical params but `PEAK_DD_PAUSE_PCT=1.0` (disabled) produced PnL=$3,314, 238 trades, 59.2% WR, peak $4,780, closely matching live results.

Conclusion: sweep results are technically correct for v0.5 code, but the peak DD pause must be disabled for parameter optimization sweeps. See sweep 003.
