# Sweep 001, INVALID (stale temp DB contamination)

Date: 2026-03-13
Status: INVALID, results inflated by stale temp databases

## Parameters

- Data: `data/market-data.db` (runs 004-007)
- Time range: 2026-02-20T03:13 -> 2026-02-28T00:00 (188.8h)
- Balance: $200
- Swept:
  - `LATENCY_ARB_MOMENTUM_THRESHOLD`: 0.001 -> 0.003, step 0.0002 (11 values)
  - `LATENCY_ARB_MAX_ASK`: 0.45, 0.50, 0.55, 0.60, 0.65 (5 values)
  - `MAX_POSITION_FRACTION`: 0.05, 0.075, 0.10, 0.125, 0.15 (5 values)
- Fixed: `SPREAD_CAPTURE_THRESHOLD=0.50` (disables spread-capture)
- Total: 275 combinations

## Command

```bash
npm run sweep -- \
  --data data/market-data.db \
  --output data/sweeps/001/sweep.csv \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50
```

## Top 5 by PnL (inflated, do not trust)

| mom | ask | frac | PnL | WR | Trades | MaxDD |
|---|---|---|---|---|---|---|
| 0.0012 | 0.60 | 0.15 | $14,602 | 56.9% | 450 | 51.1% |
| 0.0012 | 0.60 | 0.125 | $14,069 | 56.9% | 450 | 50.7% |
| 0.0012 | 0.65 | 0.125 | $12,656 | 57.4% | 514 | 53.4% |
| 0.0012 | 0.65 | 0.15 | $12,533 | 57.4% | 514 | 57.8% |
| 0.0012 | 0.60 | 0.10 | $10,385 | 56.9% | 450 | 49.3% |

## Why invalid

Temp DB path was `backtest/results/sweep-NNNN.db`. Stale `.db` files from earlier test sweeps (`test-sweep.csv`, `test-sweep-2.csv`) remained at that path. The `BankrollManager` constructor recovers balance from existing DB (`db.getLatestBalance()`), so each sweep iteration started with inflated balance from the previous test run rather than $200. Evidence:

- Trade counts (301, 450, 514) far exceed the 107-238 range produced by clean runs with the same parameters
- A standalone backtest with identical params (0.001, 0.45, 0.05) produces PnL=-$9.58 / 107 trades, matching sweep 002, not sweep 001
- HWM values ($18,977 for top combo) are impossible from $200 start in 189h without balance inflation
