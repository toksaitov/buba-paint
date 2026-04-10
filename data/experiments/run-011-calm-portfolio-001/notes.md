# Run 011 Calm Portfolio 001, combined confirmation with latency-arb and spread unchanged

This pass asks the only question that matters before a calm release candidate:

Does the calm fix actually improve the combined portfolio on the exact `run-011` tape, without changing latency-arb or spread behavior when calm is off?

The shared portfolio settings are held constant. Only the calm ask cap and the new calm expected-edge floor move.

## Combined replay results

`combined.tsv` contains the raw rows. The important comparisons are:

- baseline current calm `0.75 / 0.00`
  - total `+$94.44`
  - `52` total trades
  - latency `+$128.05` on `21` trades
  - calm `-$33.62` on `31` trades
  - calm rejected `1,604`
  - max DD `32.5%`

- candidate `0.60 / 0.00`
  - total `+$93.32`
  - calm `-$31.56` on `14` trades
  - calm rejected `1,086`
  - max DD `31.3%`

- candidate `0.65 / 0.00`
  - total `+$114.84`
  - calm `-$16.95` on `15` trades
  - calm rejected `1,156`
  - max DD `30.2%`

- candidate `0.65 / 0.05`
  - total `+$158.61`
  - `41` total trades
  - latency `+$154.64` on `22` trades
  - calm `+$3.97` on `19` trades
  - calm rejected `52`
  - max DD `29.4%`

The winner is not close. `0.65 / 0.05` is the only tested calm row that turns calm positive on this exact live tape while also improving the whole portfolio materially.

## Calm-disabled parity check

`calm_disabled.tsv` verifies that the new calm knobs do not perturb the other families when calm is disabled:

- calm off baseline -> `+$161.89`, `22` latency trades, `0` spread trades
- calm off candidate -> `+$161.89`, `22` latency trades, `0` spread trades

That is the expected result. The calm changes are isolated.

## Takeaway

The next calm point-release candidate is:

- `CALM_PERSISTENCE_MAX_ASK=0.65`
- `CALM_PERSISTENCE_MIN_EXPECTED_EDGE=0.05`

with the rest of the calm row unchanged:

- `30-90s` window
- `MIN_ABS_DISTANCE_BPS=6`
- `DISTANCE_VOL_RATIO_THRESHOLD=1.0`
- `MIN_ALIGNMENT_FRACTION=0.5`
- `MAX_FAIR_BIAS=0.35`
- `MAX_REALIZED_VOL_15S_BPS=80`
- `MAX_OPEN_CROSSES_30S=1`
- `MAX_QUOTE_CHURN_PER_S=100`
- `MAX_POSITION_FRACTION=0.05`

This row is not the raw best calm-only row on every dataset. It is the best combined release candidate after weighing:

- the `run-011` exact forensic replay
- the post-fix historical calm sweep
- calm-disabled parity for latency-arb and spread
