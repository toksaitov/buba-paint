# Pending-Settlement Reserve Modes

This chapter documents how the bot reserves capital after a market has closed but before authoritative Polymarket settlement is known.

## Purpose

Market risk ends at window close. Settlement accounting can arrive later. The reserve model separates those two states so the bot can avoid treating closed unresolved trades as fully active positions forever.

The model distinguishes:

* `active_market`: the market is still open.
* `pending_settlement`: the market is closed, but Gamma has not resolved it yet.

This matters for exact-run replay, live capital accounting, and strategy sleeve pressure.

## Public Knobs

The public env interface is:

* `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION`
* `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION`
* `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION`
* `BACKTEST_SETTLEMENT_MODE`

`BACKTEST_SETTLEMENT_MODE` controls when replay settles trades:

* `immediate`: settle at market close. Use this as the broad historical fallback.
* `observed_market_resolution`: keep trades pending until the observed authoritative resolution timestamp from the pulled run.

The reserve triple controls how much capital and position pressure remain while a trade is in `pending_settlement`.

## Named Modes

The code classifies the reserve triple into one named mode for logs and diagnostics.

* `compatibility`: `1.0 / 1.0 / true`
* `conservative`: `0.0 / 1.0 / false`
* `risky`: `0.0 / 0.25 / false`
* `custom`: any other valid combination

Meanings:

* family fraction: how much strategy-family sleeve remains occupied after market close
* global fraction: how much account-level reserve remains locked after market close
* counts as open position: whether pending-settlement trades still consume open-position slots

## Current Default

The current default is the run-012-style `risky` profile:

```bash
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false
```

This means:

* the strategy-family sleeve is released at market close
* 25% of the global reserve remains locked until authoritative settlement
* pending-settlement trades do not count as open positions

This profile is used by the current Docker `live_readonly` configuration and is the documented future latency-only pilot baseline. It is more aggressive than the conservative profile and must not be treated as permission to trade real money.

## Mode Selection

Use `compatibility` for legacy comparison:

* reproducing old behavior
* diagnosing replay divergence
* proving whether a reserve change altered historical behavior

Use `conservative` for safety comparison:

* measuring the effect of keeping the global reserve fully locked
* checking whether reduced reserve pressure creates unacceptable exposure

Use the default `risky` profile for current readonly deployment and run-012-style latency-only replay:

* family sleeve releases at close
* global reserve keeps a 25% haircut until settlement
* open-position slots are released after close

## Exact-Run Replay

For exact pulled-run calibration, prefer observed resolution timing:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
cargo run -p buba-paint --release -- backtest \
  --data /tmp/run-replay-data.db \
  --start 2026-04-04T20:15 \
  --end 2026-04-08T17:25 \
  --balance 100 \
  --set LATENCY_ARB_ENABLED=true \
  --set SPREAD_CAPTURE_ENABLED=false \
  --set CALM_PERSISTENCE_ENABLED=false
```

The default reserve profile already supplies the current reserve triple. Override it only when intentionally comparing modes.

Compatibility replay example:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0 \
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=true \
cargo run -p buba-paint --release -- backtest ...
```

Boolean env vars and boolean `--set` overrides accept `true/false`, `1/0`, `yes/no`, and `on/off`. Operator docs should prefer `true/false`.

## Historical Run-018 Note

Run 018 parity work produced a historical candidate block:

```bash
LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008
LATENCY_ARB_MAX_ASK=0.60
LATENCY_ARB_MAX_POSITION_FRACTION=0.075
SPREAD_CAPTURE_THRESHOLD=0.970
PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0
PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0
PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false
```

That block remains provenance. It is not the current deployment profile and is not a live-money promotion. Current remote deployment uses the run-012-style latency-only row with `LATENCY_ARB_MAX_POSITION_FRACTION=0.125` and `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=0.25`.

## Operational Notes

* reserve fractions must be within `[0.0, 1.0]`
* invalid values fail fast at startup and before backtests or sweeps
* live startup rebuilds unresolved reserve state from the DB
* the live bot logs the resolved pending-settlement mode and reserve fractions at startup
* backtests and sweeps inherit env-backed reserve and settlement settings through `Config::from_env()`

## Related Files

* [../bots/paint/src/config.rs](../bots/paint/src/config.rs)
* [../bots/paint/src/bankroll.rs](../bots/paint/src/bankroll.rs)
* [../bots/paint/src/live.rs](../bots/paint/src/live.rs)
* [../bots/paint/src/backtest/runner.rs](../bots/paint/src/backtest/runner.rs)
* [../data/experiments/run-018-parity-002/notes.md](../data/experiments/run-018-parity-002/notes.md)
* [../data/sweeps/run-018-003/notes.md](../data/sweeps/run-018-003/notes.md)
