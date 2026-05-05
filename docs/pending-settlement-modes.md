# Pending-Settlement Reserve Modes

This page documents the reserve model that now sits between strategy selection and final settlement. It exists because exact-run parity and live capital usage both depend on it, and the three env knobs are too easy to forget when they are only mentioned inline.

## Why This Exists

When a market closes, directional trading risk is over, but authoritative Polymarket settlement can arrive minutes or hours later. Before the reserve rework, the bot kept treating those unresolved trades like active market risk:

- the strategy-family sleeve stayed occupied
- the global reserve stayed locked
- the trade still counted toward open-position limits

That behavior was safe, but it penalized the live bot for Gamma settlement lag. On the pulled `run-018`, every latency-arb `strategy_sleeve_exhausted` rejection happened while the blocking latency trade was already past market end.

The reserve model now distinguishes two unresolved phases:

- `active_market`: the market is still live
- `pending_settlement`: the market is closed, but Gamma has not resolved it yet

## Public Knobs

The public env interface is still the same:

- `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION`
- `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION`
- `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION`
- `BACKTEST_SETTLEMENT_MODE`

`BACKTEST_SETTLEMENT_MODE` controls when replay settles trades:

- `immediate`: settle at market close. This is still the general fallback for broad historical backtests.
- `observed_market_resolution`: for exact pulled runs, keep trades in pending settlement until the observed authoritative resolution timestamp from the live run.

The three reserve knobs control what happens while a trade is in `pending_settlement`.

## Named Modes

The code now classifies the reserve triple into one named mode for logs and docs:

- `compatibility`: `1.0 / 1.0 / true`
- `conservative`: `0.0 / 1.0 / false`
- `risky`: `0.0 / 0.25 / false`
- `custom`: anything else

Meaning:

- family fraction: how much of the strategy sleeve remains occupied after market close
- global fraction: how much of the global reserve remains locked after market close
- counts as open position: whether pending-settlement trades still consume open-position slots

## Real Default

The real default is now the `risky` run-012 profile, not `compatibility` or `conservative`.

That means:

- `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
- `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25`
- `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false`

In practice:

- the family sleeve is released at market close
- 25% of the account-level reserve stays locked until authoritative settlement
- pending-settlement trades stop counting toward open-position caps

This is the selected live-readonly and first-canary baseline because it matches the run-012 latency sleeve we decided to carry forward. It is intentionally more aggressive than the conservative reserve profile.

## When To Use Each Mode

Use `compatibility` only for legacy comparison:

- reproducing old behavior
- diagnosing why an old replay diverged from a new replay
- checking whether a code change accidentally changed semantics

Use `conservative` for safety comparison runs:

- safer than the selected canary profile
- useful for comparing whether the 25% global reserve haircut is adding unacceptable exposure
- not the current deployment default

Use `risky` for the selected run-012 latency-only canary baseline:

- family sleeve still releases at close
- global reserve lock is reduced while waiting for settlement
- matches the old run knobs the current Docker deployment is expected to use

## Exact-Run Parity Workflow

For an exact pulled live run such as [runs/010/run-018-live.db](/Users/toksaitov/Desktop/buba-paint/runs/010/run-018-live.db), the preferred replay mode is:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
cargo run -p buba-paint --release -- backtest \
  --data /tmp/run-018-replay-data.db \
  --start 2026-04-04T20:15 \
  --end 2026-04-08T17:25 \
  --balance 100 \
  --set LATENCY_ARB_ENABLED=true \
  --set SPREAD_CAPTURE_ENABLED=false \
  --set CALM_PERSISTENCE_ENABLED=false
```

Because the selected canary profile is now the default, those three reserve knobs do not need to be repeated unless you are intentionally overriding them.

Boolean env vars and boolean `--set` overrides accept `true/false`, `1/0`, `yes/no`, and `on/off`, but operator examples should prefer `true/false`.

Use `compatibility` only if you are proving a regression:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=true \
cargo run -p buba-paint --release -- backtest ...
```

The current default reserve profile is:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false \
cargo run -p buba-paint --release -- backtest ...
```

## Historical Run-018 Candidate Block

After the exact `run-018` parity work and the parity-aware sweep, this candidate block was useful historical evidence:

```bash
LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008
LATENCY_ARB_MAX_ASK=0.60
LATENCY_ARB_MAX_POSITION_FRACTION=0.075
SPREAD_CAPTURE_THRESHOLD=0.970
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false
```

That was the balanced row from the parity-aware `run-018` frontier:

- better than the currently deployed row on exact-run replay
- below the `20%` drawdown line on that run
- preferred over the more aggressive `0.10` latency sleeve row unless a later exact-run rerun moves the frontier

Do not treat this as a current live-money promotion by itself. The current deployment profile is the run-012 latency-only row with a `0.125` latency sleeve and the `0.25` global pending-settlement reserve.

## Operational Notes

- Reserve fractions must be within `[0.0, 1.0]`. Invalid values now fail fast at startup and before backtests or sweeps.
- Live startup rebuilds unresolved reserve state from the DB, so a partial restart does not forget locked capital.
- The live bot logs the resolved pending-settlement mode and reserve fractions at startup.
- Backtests and sweeps inherit env-backed reserve and settlement settings through `Config::from_env()`.

## Related Files

- [bots/paint/src/config.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/config.rs)
- [bots/paint/src/bankroll.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/bankroll.rs)
- [bots/paint/src/live.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/live.rs)
- [bots/paint/src/backtest/runner.rs](/Users/toksaitov/Desktop/buba-paint/bots/paint/src/backtest/runner.rs)
- [data/experiments/run-018-parity-002/notes.md](/Users/toksaitov/Desktop/buba-paint/data/experiments/run-018-parity-002/notes.md)
- [data/sweeps/run-018-003/notes.md](/Users/toksaitov/Desktop/buba-paint/data/sweeps/run-018-003/notes.md)
