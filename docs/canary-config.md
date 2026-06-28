# Canary Configuration And Revert

This is the exact, reversible configuration for the supervised single-order canary in
[CANARY_RUNBOOK.md](../CANARY_RUNBOOK.md). The canary is a config overlay over
unchanged code and unchanged production defaults. Nothing in the strategy, decision,
or backtest numerics is modified; reverting restores production behavior exactly.

## What changed in code (committed, inert by default)

These knobs were added but default to off, so production behavior is unchanged
unless the canary overlay sets them:

* `LIVE_DRY_RUN` (default false): build the order intent but never call the venue.
* `LIVE_MAX_SESSION_ORDERS` (default 0, unlimited): structural cap on venue
  submission attempts per session, enforced atomically in the reservation.
* `LIVE_MAX_SESSION_FILLS` (default 0, unlimited): stop further submissions once
  this many orders in the session have filled (a fill is `accepted_size > 0` on the
  order row, written synchronously when the venue response is handled), checked
  atomically in the same reservation. With the order cap this gives "a few attempts
  to land one fill, then stop." At-most-one-fill is a layered guarantee, not the fill
  cap alone: the `LIVE_MAX_OPEN_NOTIONAL_USD=5` ceiling lets only one 5 USD
  reservation be in flight at a time (a filled order keeps its reservation committed
  until settlement), so the next order cannot reserve until the prior fill is already
  recorded, at which point the fill cap blocks it. The fill cap, the single in-flight
  reservation, and the bot's sequential submission together bound the canary to one
  fill.

No strategy or decision code or default constant changed. `git diff origin/master --
bots/paint/src/strategies bots/paint/src/decision` stays empty.

## The canary overlay (pure config; production default in parentheses)

Set these as environment variables or `--set KEY=VALUE` for the canary run only.
Each lists the production default to revert to.

Safety and mode:

* `EXECUTION_MODE=live_trading` (deployment default: `live_readonly`)
* `LIVE_DRY_RUN=true` for the rehearsal, `false` for the real canary (default false)
* `LIVE_MAX_SESSION_ORDERS=3` (default 0): at most three venue attempts to land one fill
* `LIVE_MAX_SESSION_FILLS=1` (default 0): stop after the first fill
* `LIVE_SESSION_CASH_CAP_USD=5` (default 100)
* `LIVE_MAX_SINGLE_ORDER_USD=5` (default 10)
* `LIVE_MAX_OPEN_NOTIONAL_USD=5` (default 25)
* `LIVE_MAX_DAILY_LOSS_USD=5` (default 15)
* `LIVE_MAX_SESSION_DRAWDOWN_USD=5` (default 20)
* `LIVE_EXPECTED_SIGNATURE_TYPE=1` and `LIVE_EXPECTED_EGRESS_IP=<server egress>`

Strategy relaxation (only to make a real signal fire on a quiet day; reverts to the
production values below):

* `LATENCY_ARB_ENABLED=true`, with `SPREAD_CAPTURE_ENABLED=false` and
  `CALM_PERSISTENCE_ENABLED=false` so only the validated latency-arb path is live.
* `LATENCY_ARB_MOMENTUM_THRESHOLD` (default `0.0008`): the base trigger. The
  effective threshold is `max(base, p85 of recent momentum)`, so on a quiet day the
  base is what blocks signals. Choose the canary value from data: read the recent
  captured momentum on a comparable quiet period and set the base just below a level
  that occurs every few minutes, so a real, small signal fires within the test
  window. Do not hardcode a guess; pick it from recent capture at canary time.
* `LATENCY_ARB_COOLDOWN_MS` (default `60000`): optionally lower (for example
  `15000`) so a no-fill can retry within the five-minute window, bounded by
  `LIVE_MAX_SESSION_ORDERS`.
* `LATENCY_ARB_MAX_ASK` (default `0.60`) and `LATENCY_ARB_MIN_ASK` (default `0.30`):
  widen only if needed so the order is marketable enough to fill on the active
  market; for BTC 5-minute up/down the defaults are usually sufficient.

The relaxation makes the canary fire on a more sensitive trigger than the production
default. Everything else stays real: real feed, real book, real features, real
strategy math, real decision, real marketable order, real fill, real on-chain
reconciliation, real settlement, real redeem, real fee.

## Defense in depth on exposure

Reduce the on-chain allowance and spendable balance to about 5 USD plus dust before
the real canary (or use a fresh isolated wallet funded with about 5 USD), so the
venue cannot pull more than the canary size regardless of any software fault. The
software caps above are defense in depth, not the primary ceiling.

## Revert (bring back the real strategy)

1. Stop the canary run and remove the overlay: run with the production config and
   none of the canary environment variables or `--set` overrides.
2. With the overlay gone, the committed defaults apply unchanged:
   `LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008`, `LATENCY_ARB_MAX_ASK=0.60`,
   `LATENCY_ARB_MIN_ASK=0.30`, `LATENCY_ARB_COOLDOWN_MS=60000`, all caps off
   (`LIVE_MAX_SESSION_ORDERS=0`, `LIVE_MAX_SESSION_FILLS=0`), and
   `LIVE_DRY_RUN=false`. Production strategy behavior is restored exactly.
3. The added code knobs stay inert at their defaults, so there is no residual effect.
4. Use a separate run DB for the canary. Archive it after; start any later run from a
   clean DB, since `live_trading` refuses to start against a DB with an unconfirmed
   fill or a prior halt.
