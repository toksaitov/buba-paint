# Sweep rust-013, release-readiness rerun on the rebuilt merged DB

`rust-013` is the release-readiness rerun of `rust-012`. It uses the same merged historical dataset, the same simulator and feature-engine code, and the same parameter grid. The point of this rerun was not to search a new frontier. It was to confirm that the current release-candidate tree still reproduces the established frontier after the local/remote smoke tests and the fresh `data/market-data.db` rebuild.

## Command

```bash
./target/release/buba-paint sweep \
  --data data/market-data.db \
  --start 2026-02-15T19:10 --end 2026-03-31T19:35 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012,0.0014,0.0016,0.0018 \
  --sweep LATENCY_ARB_MAX_ASK=0.50,0.55,0.60,0.65,0.70 \
  --sweep MAX_POSITION_FRACTION=0.03,0.05,0.075,0.10,0.125 \
  --sweep SPREAD_CAPTURE_THRESHOLD=0.965,0.970,0.975,0.980,0.985 \
  --set TAKER_FEE_RATE=0.072 \
  --set TAKER_FEE_EXPONENT=1 \
  --set SIM_ORDER_LATENCY_MS=250 \
  --output data/sweeps/rust-013/sweep.csv
```

## Result

The important result is that `rust-013` is functionally identical to `rust-012`.

- Changed rows versus `rust-012`: `0 / 750`
- Maximum difference in `pnl_net`, `max_dd`, `win_rate`, or `trades`: `0`
- Positive rows: `497 / 750`
- Mean `pnl_net`: `$2,990.68`

This is the right outcome for a release-readiness rerun. The historical rebuild, local smoke, and remote smoke did not change the backtest frontier.

## Top raw PnL rows

These are unchanged from `rust-012`.

1. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.965`: `$37,398`, `71.7%` WR, `435` trades, `34.0%` DD, `53.2%` fill rate, `75` legging events
2. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.970`: `$37,309`, `71.9%` WR, `437` trades, `34.0%` DD, `52.0%` fill rate, `77` legging events
3. mom=`0.0008`, ask=`0.60`, frac=`0.10`, spread=`0.965`: `$35,431`, `71.8%` WR, `432` trades, `33.6%` DD, `52.4%` fill rate, `72` legging events
4. mom=`0.0008`, ask=`0.60`, frac=`0.10`, spread=`0.970`: `$35,398`, `71.9%` WR, `434` trades, `33.6%` DD, `51.3%` fill rate, `74` legging events
5. mom=`0.0008`, ask=`0.60`, frac=`0.125`, spread=`0.975`: `$34,803`, `68.9%` WR, `463` trades, `36.1%` DD, `49.5%` fill rate, `109` legging events

These remain too aggressive for the next paper-live test. The raw-return leaders are still driven by `frac=0.10` and `frac=0.125`, and their drawdowns are still too deep for a cautious release candidate.

## Balanced and strict-DD shortlist

The release candidate still points to the same safer row as `rust-012`.

- Safest strong row: mom=`0.0008`, ask=`0.65`, frac=`0.05`, spread=`0.970` -> `$25,335`, `74.2%` WR, `454` trades, `19.2%` DD, `43.6%` fill rate, `55` legging events
- Close strict-DD alternative: mom=`0.0008`, ask=`0.65`, frac=`0.05`, spread=`0.965` -> `$24,230`, `73.8%` WR, `443` trades, `19.2%` DD
- More aggressive balanced row: mom=`0.0008`, ask=`0.60`, frac=`0.05`, spread=`0.970` -> `$28,043`, `73.3%` WR, `412` trades, `20.7%` DD
- Lowest-drawdown useful row: mom=`0.0008`, ask=`0.65`, frac=`0.03`, spread=`0.970` -> `$11,719`, `78.4%` WR, `412` trades, `11.8%` DD

## Comparison to rust-012

`rust-013` is a reproducibility confirmation, not a new regime shift.

- Same top raw-PnL row
- Same balanced frontier
- Same strict-DD leader
- Same fill, legging, and trade counts on every parameter row

Only elapsed runtime changed. The strategy metrics did not.

## Fidelity caveat

This historical dataset is still legacy-only.

- `raw_event_batches=0` on every row
- `legacy_snapshot_batches=3,002,577` on every row

So `rust-013` does not yet answer whether the future raw-event live-paper history will materially change the frontier. It only confirms that the current release tree is stable on the rebuilt historical universe.

## Deployment recommendation

For the next paper-live collection run, the recommended release candidate remains:

- `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
- `LATENCY_ARB_MAX_ASK=0.65`
- `MAX_POSITION_FRACTION=0.05`
- `SPREAD_CAPTURE_THRESHOLD=0.970`
- `TAKER_FEE_RATE=0.072`
- `TAKER_FEE_EXPONENT=1`
- `SIM_ORDER_LATENCY_MS=250`

Reasoning:

- It is still the best strong row under `20%` max drawdown.
- It preserves healthy trade count and fill rate.
- It avoids the overexposed `frac=0.10` and `frac=0.125` frontier.
- `rust-013` confirms that this recommendation still holds after the release-readiness rebuild and runtime smoke tests.
