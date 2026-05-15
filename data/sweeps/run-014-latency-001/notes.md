# Run 014 latency-only sweep notes

## Inputs

- Source run: `runs/014/server-20260511-171249/paint.db`
- Prepared DB: `data/sweeps/run-014-latency-001/prepared.db`
- Interval: `2026-05-11T17:25:00Z` to `2026-05-12T12:00:00Z`
- Mode: latency-arb only, starting balance `$100`
- Replay quality: `sweep_grade`
- Backtest input: `backtest_ready`
- Prepared rows: `6,903,926` generic feed rows and `52,863,533` compact CLOB rows

## Baseline exact replay

Run 012-style deployed latency settings replayed cleanly:

- `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
- `LATENCY_ARB_MAX_ASK=0.60`
- `LATENCY_ARB_MAX_POSITION_FRACTION=0.125`
- `LATENCY_ARB_COOLDOWN_MS=60000`

Result:

- Ticks: `59,767,459`
- Windows: `223`
- Signals: `5`
- Trades/results: `3`
- PnL: `$26.07`
- Win rate: `100%`
- Max drawdown: `0.0%`
- Final balance: `$126.07`

This matches the live-readonly shadow result shape from the run DB: `5` signals and `3` simulated trades/results.

## Sweep result

The sweep completed all `600` rows with no error lines in `sweep.log`.

- Positive rows: `72`
- Rows with trades: `81`
- Negative rows: `9`
- Zero-PnL rows: `519`
- Runtime: `18,058.1s` (`~5h 1m`)

Best raw row:

- `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0006`
- `LATENCY_ARB_MAX_ASK=0.60`
- `LATENCY_ARB_MAX_POSITION_FRACTION=0.125`
- `LATENCY_ARB_COOLDOWN_MS=30000/60000/120000` all tied
- PnL: `$34.08`
- Trades: `6`
- Win rate: `83.3%`
- Max drawdown: `11.6%`

Best zero-drawdown row:

- `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
- `LATENCY_ARB_MAX_ASK=0.70`
- `LATENCY_ARB_MAX_POSITION_FRACTION=0.125`
- `LATENCY_ARB_COOLDOWN_MS=30000/60000/120000` all tied
- PnL: `$32.33`
- Trades: `5`
- Win rate: `100%`
- Max drawdown: `0.0%`

Deployed row rank:

- Settings: `0.0008 / 0.60 / 0.125 / 60000`
- Rank: `20/600` by PnL ordering, with adjacent cooldown variants tied
- PnL: `$26.07`
- Trades: `3`
- Win rate: `100%`
- Max drawdown: `0.0%`

## Interpretation

The current deployed latency settings behaved correctly and safely on this interval. The exact replay matched the live-readonly shadow outcome, which is the most important result.

The best raw sweep row improved PnL by taking one losing trade and accepting `11.6%` drawdown. The best zero-drawdown row raised `LATENCY_ARB_MAX_ASK` to `0.70`, but it only has `5` trades, so it is not enough evidence to promote.

`LATENCY_ARB_MOMENTUM_THRESHOLD >= 0.0010` produced no trades. Position fractions below `0.075` also produced no trades, likely due minimum-order sizing constraints on a `$100` balance. Cooldown did not matter in this interval because opportunities were sparse.

The risky corner is `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0006` with `LATENCY_ARB_MAX_ASK=0.70`: all negative rows came from that combination.

Recommendation: do not promote a new parameter set from this one `18.6h` interval. Keep the current canary settings (`0.0008`, `0.60`, `0.125`, `60000`) unless another independent run confirms that `MAX_ASK=0.70` at threshold `0.0008` is robust.
