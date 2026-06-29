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
  atomically in the same reservation. With `LIVE_MAX_SESSION_ORDERS=1` this gives
  "one venue submission, then stop, whether it fills or not." At-most-one-fill is a
  layered guarantee, not the fill
  cap alone: the `LIVE_MAX_OPEN_NOTIONAL_USD=7` ceiling, which is below twice a
  minimum order, lets only one reservation be in flight at a time (a filled order
  keeps its reservation committed until settlement), so the next order cannot reserve
  until the prior fill is already recorded, at which point the fill cap blocks it. The
  fill cap, the single in-flight reservation, and the bot's sequential submission
  together bound the canary to one fill.

No strategy or decision code or default constant changed. `git diff origin/master --
bots/paint/src/strategies bots/paint/src/decision` stays empty.

## The canary overlay (pure config; production default in parentheses)

Set these as environment variables or `--set KEY=VALUE` for the canary run only.
Each lists the production default to revert to. On the Docker host the overlay is
applied with `scripts/deploy-docker.py --mode live-trading`, which uses
`docker-compose.live-trading.yml` (dry-run, disarmed, and the canary caps with the
7 USD ceiling are the safe defaults there) plus `--env-set KEY=VALUE` for the values
that change per run
(`LIVE_DRY_RUN`, the data-chosen threshold, the egress pin). Reverting is a
redeploy with `--mode live-readonly`.

Safety and mode:

* `EXECUTION_MODE=live_trading` (deployment default: `live_readonly`)
* `LIVE_DRY_RUN=true` for the rehearsal, `false` for the real canary (default false)
* `LIVE_MAX_SESSION_ORDERS=1` (default 0): exactly one venue submission per session;
  the canary places one order and does not retry a no-fill (matches the runbook scope
  "no retry after a no-fill"). A no-fill leaves no position; to try again, start a new
  session on a fresh run DB.
* `LIVE_MAX_SESSION_FILLS=1` (default 0): stop after the first fill
* `LIVE_ARMED_FEED_OUTAGE_HALT_MS=120000` (default 120000): while armed, halt if a
  critical decision feed (Binance or CLOB) is in a continuous outage past this many
  ms; brief reconnect blips only block orders, a sustained outage halts the session
* `LIVE_SESSION_CASH_CAP_USD=100` (default 100): the sizing bankroll, kept at the
  production value so the order sizes naturally to about 5 USD (see sizing note)
* `LIVE_MAX_SINGLE_ORDER_USD=7` (default 10): hard pre-submit ceiling on one order
* `LIVE_MAX_OPEN_NOTIONAL_USD=7` (default 25): the binding exposure ceiling
* `LIVE_MAX_DAILY_LOSS_USD=7` (default 15)
* `LIVE_MAX_SESSION_DRAWDOWN_USD=7` (default 20)
* `LIVE_MIN_REQUIRED_CASH_USD=5` (default 25): preflight cash floor, must be <= cash cap
* `LIVE_EXPECTED_SIGNATURE_TYPE=1` and `LIVE_EXPECTED_EGRESS_IP=<server egress>`

Sizing note (why the cash cap stays at 100, not 5): the position-fraction knobs are
production values (`MAX_POSITION_FRACTION=0.05`, `LATENCY_ARB_MAX_POSITION_FRACTION=0.125`,
`MAX_POSITION_USD_FRACTION=0.20`). A single order is sized as bankroll times these
fractions, so a roughly 5 USD order needs a bankroll near 100 USD; on a 5 USD
bankroll the fractions starve the order below the 5 USD minimum bet and nothing is
ever queued. Keeping the bankroll at 100 with the production fractions reproduces the
exact production sizing (about 5 USD), and exposure is bounded instead by
`LIVE_MAX_OPEN_NOTIONAL_USD=7` and `LIVE_MAX_SINGLE_ORDER_USD=7`. The 7 (not 5)
accounts for whole-share rounding: a 5 USD min-bet order buys `ceil(5 / price)` shares,
which costs slightly more than 5 USD (for example 10 shares at 0.52 is 5.20 USD). The
ceiling stays below twice the minimum order, so only one reservation is ever in flight,
preserving the at-most-one-fill guarantee.

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
* `LATENCY_ARB_COOLDOWN_MS` (default `60000`): for the dry-run rehearsal, optionally
  lower (for example `15000`) so a would-submit fires sooner. For the real canary with
  `LIVE_MAX_SESSION_ORDERS=1` there is no retry, so the cooldown only affects how soon
  the single eligible signal can fire.
* `LATENCY_ARB_MAX_ASK` (default `0.60`) and `LATENCY_ARB_MIN_ASK` (production
  `0.30`, canary `0.50`): the canary narrows the ask band to `0.50..0.60`. The bot
  sizes the order at the current ask but reserves capital at the limit price
  (`LATENCY_ARB_MAX_ASK=0.60`), so a low ask needs many shares to reach the 5 USD
  minimum bet and the reservation at 0.60 would exceed the 7 USD ceiling and reject
  the order before it queues. With `MIN_ASK=0.50` the worst case is
  `ceil(5 / 0.50) = 10` shares reserved at 0.60, which is 6.00 USD and fits under 7,
  while `2 x` the smallest reservation (about 5.40 USD) stays above 7, so only one
  reservation is ever in flight. This narrows what counts as marketable but does not
  change what the canary validates (a real order on a near-even BTC 5-minute market).

The relaxation makes the canary fire on a more sensitive trigger than the production
default. Everything else stays real: real feed, real book, real features, real
strategy math, real decision, real marketable order, real fill, real on-chain
reconciliation, real settlement, real redeem, real fee.

## Defense in depth on exposure

The primary ceiling is software, layered across independent components: the bot
rejects any order over `LIVE_MAX_SINGLE_ORDER_USD=7`, the sidecar independently
rejects any BUY over `SIDECAR_MAX_ORDER_USD` (8 for the canary) at the venue boundary,
the open-notional ceiling bounds total exposure, the fill cap stops after one fill,
and bootstrap fails closed on any prior submission so a restart cannot place a second
order. These were reviewed and signed off for the canary.

Do not reduce the spendable balance: the production position-fraction sizing needs a
bankroll near 100 USD to size a 5 USD order, so a small balance would starve the order
entirely (see the sizing note above). Keep the real balance.

Reducing the on-chain CTF exchange allowance (the ERC-20 approval, not the balance) to
about 8 to 10 USD is an optional extra backstop, not a precondition. On the current
account it is impractical: the funds are in a Magic-link proxy wallet, which is not a
normal wallet that connects to a token-approval tool, and Polymarket does not expose a
raw allowance setting. Only if a fresh isolated wallet with a directly settable
allowance is used does capping its allowance near 8 to 10 USD become the strongest
single backstop; it is otherwise not required given the software ceilings.

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
