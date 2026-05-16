# Sweep rust-008, reality-adjusted (liquidity clamping + dynamic fees)

Date: 2026-03-27
Status: VALID, first sweep with production-grade constraints

## Purpose

Re-ran the rust-007 parameter grid with v0.7 improvements: order book liquidity clamping (trade sizes capped to available ask_size), MAX_POSITION_USD=$500 hard cap, and Polymarket's dynamic taker fee model (fee_rate=0.25, exponent=2, peak 1.56% at $0.50 entry).

This is the first sweep that produces results comparable to what real money would achieve.

## Parameters

* Data: `data/market-data.db` (runs 004-008)
* Time range: 2026-02-15 to 2026-03-27 (~928h, ~39 days)
* Ticks: 11,074,264
* Balance: $200
* Swept:
  * `LATENCY_ARB_MOMENTUM_THRESHOLD`: 0.0008, 0.0010, 0.0012, 0.0014, 0.0016, 0.0018 (6 values)
  * `LATENCY_ARB_MAX_ASK`: 0.50, 0.55, 0.60, 0.65, 0.70 (5 values)
  * `MAX_POSITION_FRACTION`: 0.03, 0.05, 0.075, 0.10, 0.125 (5 values)
* Fixed:
  * `SPREAD_CAPTURE_THRESHOLD=0.50` (disables spread-capture)
  * `PEAK_DD_PAUSE_PCT=1.0` (disables peak drawdown pause)
  * `MAX_POSITION_USD=500.0` (hard cap, default)
  * `TAKER_FEE_RATE=0.25` (Polymarket crypto, default)
  * `TAKER_FEE_EXPONENT=2` (Polymarket crypto, default)
* Total: 150 combinations
* Runtime: 180s (~3 min)

## Command

```bash
cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-15 --end 2026-03-27 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-008/sweep.csv
```

## Top 10 by PnL

1. mom=0.0008, ask=0.55, frac=0.05: $88,000, 61.4% WR, 36.5% DD
2. mom=0.0008, ask=0.60, frac=0.075: $82,896, 60.5% WR, 42.6% DD
3. mom=0.0008, ask=0.60, frac=0.10: $82,136, 60.3% WR, 46.4% DD
4. mom=0.0008, ask=0.60, frac=0.05: $81,798, 60.4% WR, 33.2% DD
5. mom=0.0008, ask=0.55, frac=0.03: $81,283, 61.7% WR, 29.6% DD
6. mom=0.0012, ask=0.65, frac=0.125: $58,075, 64.1% WR, 36.6% DD
7. mom=0.0012, ask=0.6, frac=0.1: $55,836, 64.4% WR, 39.5% DD
8. mom=0.0012, ask=0.6, frac=0.125: $55,429, 63.7% WR, 39.6% DD
9. mom=0.0012, ask=0.6, frac=0.05: $54,145, 64.2% WR, 29.7% DD
10. mom=0.0008, ask=0.5, frac=0.05: $51,825, 59.7% WR, 33.2% DD

## Comparison with rust-007 (no constraints)

For run 008's live parameters (mom=0.0012, ask=0.60, frac=0.05):

* rust-007: $316M PnL, 1305 trades, 30.5% DD
* rust-008: $54k PnL, 1060 trades, 29.7% DD

The $316M was pure fiction. $54k is what the strategy could realistically produce with real money over 40 days, given order book constraints and Polymarket's fee structure.

245 trades (19%) were filtered out by the liquidity clamp (ask_size too small at signal time or capped below MIN_BET_USD after clamping).

## Key findings

1. The strategy still shows meaningful edge after fees and liquidity constraints. $54k from $200 in 40 days (27,000% return) is exceptional, though it benefits from aggressive half-Kelly compounding on a small starting balance.

2. mom=0.0008 now dominates (was 0.0012 in all previous sweeps). Lower threshold = more signals = more chances to compound. With position sizes capped by liquidity, the per-trade risk is bounded regardless.

3. Win rates are slightly higher in rust-008 (64% vs 63%) because the liquidity filter preferentially skips trades in thin-book conditions.

4. DD is similar between rust-007 and rust-008, confirming the liquidity clamp doesn't change the risk profile, just the scale.

5. The fee impact is modest at small trade sizes. At $500 per trade with a $0.50 entry price, the fee is ~$7.81 (1.56%). This matters but doesn't kill the edge when the win rate is 60%+.

6. The backtester still uses Chainlink-derived settlement (94.6% accurate vs Polymarket). The remaining 5.4% error rate means real win rates would be ~3 percentage points lower. A 61% backtested WR maps to ~58% real WR, still above the ~53% breakeven after fees.

7. Caveat: these results assume every signal gets filled at the best ask price. In reality, competition from other bots and latency would reduce fill rates further.
