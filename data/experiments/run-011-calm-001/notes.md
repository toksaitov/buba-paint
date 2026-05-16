# Run 011 Calm 001, exact live-tape forensic replay

This pass isolates `calm-persistence` on the pulled `run-011` tape after fixing two calm-specific issues:

* the shared single-order executor now uses `CALM_PERSISTENCE_MAX_ASK` for calm instead of inheriting `LATENCY_ARB_MAX_ASK`
* calm duplicate pending/open-position attempts are rejected before signal persistence

The replay uses the exact `run-011` interval and `BACKTEST_SETTLEMENT_MODE=observed_market_resolution` with the same risky pending-settlement reserve mode as the live run.

## Live reference

Before the fix, the live snapshot looked like this:

* `pnl_net`: `-$53.28`
* `trades`: `8`
* `fills`: `8`
* `misses`: `194`
* `rejected`: `25,440`
* duplicate persisted rows:
  * `duplicate_open_position = 14,508`
  * `duplicate_pending_order = 10,932`

That was the calm spam problem in plain numbers.

## Replay ladder

`calibration.tsv` contains the raw outputs. The important rows are:

* `MAX_ASK=0.75`, `MIN_EXPECTED_EDGE=0.00` -> `-$35.61`, `37` trades, `1,810` rejections
* `MAX_ASK=0.65`, `MIN_EXPECTED_EDGE=0.00` -> `-$24.59`, `21` trades, `583` rejections
* `MAX_ASK=0.60`, `MIN_EXPECTED_EDGE=0.00` -> `-$8.28`, `15` trades, `93` rejections
* `MAX_ASK=0.60`, `MIN_EXPECTED_EDGE=0.08` -> `-$8.24`, `15` trades, `93` rejections
* targeted follow-up `MAX_ASK=0.65`, `MIN_EXPECTED_EDGE=0.05` -> `-$3.05`, `19` trades, `176` rejections

What changed immediately:

* duplicate persisted rows fell to zero
* calm `signals` on the replay dropped to `111`
* the remaining rejected rows were no longer duplicate spam; the dominant replay rejection became `below_min_bet_on_submit`

## Interpretation

Two conclusions are clear.

First, the calm ask-cap drift was real. On the exact `run-011` tape, tightening calm from `0.75` to `0.60` removed most of the damage even before adding any new edge floor.

Second, the new `CALM_PERSISTENCE_MIN_EXPECTED_EDGE` knob is a secondary quality filter, not the primary rescue on this tape. Raising it from `0.00` to `0.08` on the `0.60` row only removed one miss and barely moved PnL. The stronger exact-run improvement came from pairing `0.65` with `0.05`, which was then confirmed in the combined portfolio replay.

## Takeaway

The exact `run-011` calm-only replay says:

* fix the executor/parity bug
* stop calm duplicate rows before persistence
* do not keep `MAX_ASK=0.75`

It does not, by itself, pick the final live row. The final choice comes from the combined confirmation in [run-011-calm-portfolio-001](../run-011-calm-portfolio-001/notes.md) plus the broad historical confirmation in [calm-004](../../sweeps/calm-004/notes.md).
