# Experiment run-018-parity-002, release-candidate check after making conservative reserve handling the real default

This rerun exists for one reason:

- confirm that the release-candidate recommendation still holds after the code cleanup, the default-mode switch, the dedicated reserve-mode docs, and the validation work

It intentionally does not repeat the whole frontier. It only reruns the exact `run-018` ladder that matters for the next release candidate.

## Source Data

- pulled live snapshot: [runs/010](/Users/toksaitov/Desktop/buba-paint/runs/010)
- replay-compatible DB: `/tmp/run-018-replay-data.db`
- interval:
  - start: `2026-04-04T20:15`
  - end: `2026-04-08T17:25`

## What Changed Since `run-018-parity-001`

The trading logic did not move materially. This rerun mainly validates the release candidate after:

- making conservative pending-settlement handling the actual default
- centralizing named reserve-mode semantics in config
- adding fail-fast validation for reserve fractions
- tightening live/backtest docs around reserve phases and exact-run parity

The expectation was that the exact-run results would remain directionally the same.

## Calibration Ladder

Results from [calibration.tsv](/Users/toksaitov/Desktop/buba-paint/data/experiments/run-018-parity-002/calibration.tsv):

- current row, immediate settlement reference:
  - `+$558.11`
  - `170` trades
  - DD `21.7%`
- current row, observed settlement + conservative default:
  - `+$544.42`
  - `169` trades
  - DD `24.9%`
- balanced candidate `0.0008 / 0.60 / 0.075` under conservative default:
  - `+$1058.09`
  - `115` trades
  - DD `19.1%`
- aggressive candidate `0.0008 / 0.60 / 0.10` under conservative default:
  - `+$1358.40`
  - `115` trades
  - DD `24.9%`
- balanced and aggressive risky reruns:
  - identical to conservative on this exact run

## Interpretation

This rerun confirms the same conclusion as the earlier parity pass:

- conservative mode remains the correct release-candidate baseline
- risky mode still has no justification on the exact `run-018` tape
- the balanced `0.0008 / 0.60 / 0.075` latency row remains the best candidate if the target is improved PnL without crossing the `20%` drawdown line

The cleanup did not move the ranking. That is good. It means the recommendation survived the code-quality pass instead of being an artifact of a transient implementation state.

## Release-Candidate Summary

If the next live deployment is based on `run-018` exact-run evidence, the release-candidate block remains:

- `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`
- `LATENCY_ARB_MAX_ASK=0.60`
- `LATENCY_ARB_MAX_POSITION_FRACTION=0.075`
- `SPREAD_CAPTURE_THRESHOLD=0.970`
- `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION=0.0`
- `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION=1.0`
- `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION=false`

## Promote / Kill

- Promote:
  - conservative reserve handling as the real default
  - the balanced `0.0008 / 0.60 / 0.075` latency row as the release-candidate live block
- Keep available but not promoted:
  - compatibility mode for legacy comparison
  - risky mode for future experiments only

## Next Step

The final exact-run frontier after the cleanup is [run-018-003](/Users/toksaitov/Desktop/buba-paint/data/sweeps/run-018-003/notes.md). That sweep is the last check before locking the next local release candidate.
