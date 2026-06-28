# Canary Runbook

Operational runbook for the single real-money canary order defined in
[LIVE_READINESS_PLAN.md](./LIVE_READINESS_PLAN.md) Phase 6. This is the only real
order in the plan. It is run operator-light: the operator gives one explicit GO and
keeps an independent abort channel, while Claude pilots control and Codex verifies
independently. Real-time safety is the bot's in-process job, not the agents'.

## Purpose And Scope

* Place exactly one minimum-size (5 USD) fill-or-kill BUY order on a live BTC
  5-minute up/down market, confirm it on-chain, let it settle, redeem it, and
  record the evidence.
* Prove the full armed path end to end with real money once, at the smallest size
  the venue allows, before any sized-trading decision.
* Out of scope: more than one order, any retry after a no-fill, any sizing, and any
  change to strategy, decision, or backtest numerics.

## Roles And Authority

* Bot (in-process): the real-time safety layer. It auto-halts instantly on an
  on-chain reconciliation mismatch, a risk or drawdown breach, a partial
  fill-or-kill, or a stale feed, and durable one-order-per-intent idempotency makes
  a double submission impossible. Neither agent can react in milliseconds; the bot
  is what does.
* Claude (control pilot): owns the run clock and is the only party that issues
  Arm. Issues Preflight, Arm, StopAfterFlat, RedeemAll, Disarm only after gates
  pass. Drives observation and analysis.
* Codex (independent verifier): read-only by default, verifies each gate from an
  independent reading of DB, bot state, venue, and on-chain balances, and may veto.
  A veto before Arm prevents Arm; a veto after Arm triggers KillSwitch. Holds
  emergency KillSwitch and CancelAll authority only. Never arms and never "fixes"
  state to make arming pass.
* Operator: gives exactly one GO that names the ticket, and retains an independent
  phone abort the whole time.

## Caps And Exposure

The canary runs with caps tuned so one minimum order is the only thing that can
exist:

* `LIVE_MAX_SESSION_FILLS=1` with `LIVE_MAX_SESSION_ORDERS=3`: a structural
  in-process latch checked atomically in the venue-attempt reservation. The order cap
  bounds attempts (a fill-or-kill no-fill leaves no position and would otherwise need
  a retry), and the fill cap blocks further submissions once a fill is recorded.
  At-most-one-fill is layered, not the fill cap alone: the `LIVE_MAX_OPEN_NOTIONAL_USD=5`
  ceiling lets only one 5 USD reservation be in flight at a time (a filled order keeps
  its reservation committed until settlement), so the next order cannot reserve until
  the prior fill is recorded, at which point the fill cap stops it. Together with the
  bot's sequential submission this bounds the canary to one fill regardless of
  operator reaction time. The caps fail closed, are durable per session, and never
  reset on a no-fill or a disarm; clearing them requires a new session and run DB.
* `LIVE_MAX_SINGLE_ORDER_USD=5`
* `LIVE_MAX_OPEN_NOTIONAL_USD=5` (the shared bankroll ceiling makes only one 5 USD
  position fit at a time)
* `LIVE_SESSION_CASH_CAP_USD=5`
* `LIVE_MAX_DAILY_LOSS_USD` and `LIVE_MAX_SESSION_DRAWDOWN_USD` set low so any
  breach halts immediately.

A 5 USD order means at most about 5 USD of spend, fill-or-kill, with the
strategy's worst-price limit and a small dust tolerance on the exact filled
notional. Maximum realistic exposure is about 5 USD (the stake, lost only if the
position resolves against us) plus pennies of fee.

Defense in depth, strongest first: the on-chain allowance and spendable balance are
reduced to about 5 USD plus dust before the canary (or a fresh isolated wallet is
used), so the venue itself cannot pull more than the canary size even if the bot
misbehaves; the fill-cap latch bounds it to one filled position;
fill-or-kill cannot leave a resting order; the bot halts on any anomaly; and there
is no leverage. The software caps alone are not treated as the hard ceiling while
the full wallet is spendable; the reduced allowance is.

## Preconditions

Shared (both the dry-run and the real canary):

* Latest live-readiness code deployed to the Ireland host and the stack healthy
  (paint, sidecar, agent, dashboard, caddy).
* `LIVE_EXPECTED_SIGNATURE_TYPE=1`, `POLYMARKET_SIGNATURE_TYPE=1`, proxy equals
  funder `0xE7C092ffa4c73EA874d8309cFC0e8915cb348616`, on-chain a deployed proxy.
* `LIVE_EXPECTED_EGRESS_IP` pinned to the host's current egress, read live, and the
  geoblock returns blocked false for Ireland.
* The canary caps above, including `LIVE_MAX_SESSION_ORDERS=3` and
  `LIVE_MAX_SESSION_FILLS=1`. See [canary-config.md](./docs/canary-config.md) for the
  full reversible overlay and revert steps.
* Preflight returns auth ok, clock ok, allowance ok, available cash at least the
  order minimum, contract version v2, collateral pUSD, and at least one BTC
  5-minute market accepting orders.
* No prior unconfirmed on-chain fill (the bot refuses to start otherwise), no open
  venue orders, session disarmed.

Dry-run only:

* `LIVE_DRY_RUN=true`, confirmed explicitly in preflight, on a separate runtime DB.

Real canary only:

* `LIVE_DRY_RUN=false`, confirmed explicitly in preflight.
* The dry-run rehearsal completed clean on this host with the same code and config
  fingerprint, differing only in `LIVE_DRY_RUN` and the runtime DB.
* The on-chain allowance and spendable balance are reduced to about 5 USD plus dust
  (or a fresh isolated wallet is funded with about 5 USD), so the venue cannot pull
  more than the canary size.

## The Dry-Run Rehearsal (step b, no real money)

The dry-run exercises the entire armed path and builds the real order intent but
never calls the sidecar order endpoint. It runs with `LIVE_DRY_RUN=true` on a
separate runtime DB. As defense in depth it also runs without venue write
credentials where possible, so the submit path is structurally unavailable, not
only flag-gated.

1. Start `live_trading` disarmed with `LIVE_DRY_RUN=true` and the canary caps.
2. Claude issues Preflight; both agents confirm the dry-run gate set is green
   (including `LIVE_DRY_RUN=true`).
3. Claude issues Arm.
4. On the next qualifying signal the bot builds the order intent, logs a loud
   would-submit line and a durable dry-run reconciliation event, releases the
   reservation, and does not contact the venue. Confirm: no venue order, no fill,
   no pending reconciliation, capital released, and the session order latch records
   the attempt.
5. Claude issues StopAfterFlat then Disarm. Confirm the session returns to a clean
   disarmed state with zero open orders and nothing unreconciled.
6. Codex independently confirms the venue saw no order and on-chain balances did
   not change.

The rehearsal must pass cleanly before any real canary.

## The Canary Runbook (one real order)

1. Freeze the ticket envelope: market family BTC 5-minute up/down, the discovered
   active market, 5 USD max spend, fill-or-kill only, worst-price limit from the
   strategy, one filled order only via `LIVE_MAX_SESSION_FILLS=1` with up to three
   attempts via `LIVE_MAX_SESSION_ORDERS=3`, the data-chosen relaxed latency-arb
   threshold from [canary-config.md](./docs/canary-config.md), config fingerprint and
   bot version recorded. The side is determined by the strategy signal that fires
   after Arm and cannot be frozen in advance; the bot only trades within this
   envelope and the latch bounds it to one fill, so verification is of the envelope
   and the single-fill outcome, not of a future side.
2. Preflight, read-only: Claude issues Preflight; both agents independently confirm
   every precondition above, including `LIVE_DRY_RUN=false` and the reduced
   allowance. Any mismatch is a no-go.
3. Operator GO: exactly one human GO that names the ticket ("BTC 5-minute, 5 USD
   max, fill-or-kill, one order only, go"). No implicit GO from earlier chat. If the
   config fingerprint, feed freshness, or clock drift changed between Preflight and
   GO, re-run Preflight; do not arm on stale verification.
4. Arm: Claude issues Arm. Codex immediately verifies the bot acknowledged, no
   extra intent or order was created, and state is armed. A missing or ambiguous
   Arm acknowledgement is treated as unsafe: Claude issues KillSwitch and confirms
   the session is halted before any further step.
5. Order: with the relaxed threshold, a real latency-arb signal fires on the live
   market and the bot submits one 5 USD fill-or-kill order. Healthy fill is exactly
   one matched order at 5 USD on the expected token, no sibling orders, open notional
   at most 5 USD. A no-fill is harmless (no position, capital released) and the bot
   may attempt again on the next signal up to `LIVE_MAX_SESSION_ORDERS`; once one
   order fills, `LIVE_MAX_SESSION_FILLS=1` stops all further submissions.
6. Contain: immediately after the order is terminal, Claude issues StopAfterFlat so
   no new position can open.
7. On-chain reconcile: require the venue trade, a transaction hash, a confirmed
   status, and the wallet token and pUSD deltas matching the expected amounts
   within dust. The bot has already auto-halted on any mismatch; on mismatch we
   stop and report. Codex verifies the on-chain balance independently of the bot.
8. Settle and redeem: wait for the 5-minute market to resolve, then Claude issues
   RedeemAll if the position is redeemable (a winning position becomes redeemable
   after resolution; a losing one is worthless).
9. Disarm and analyze: Claude issues Disarm. Final state must be disarmed, zero
   open orders, no pending trades, no unreconciled fills, and a stored snapshot of
   cash, tokens, realized PnL, and the realized fee versus the live `fd.r`.
10. Go/no-go: Claude and Codex each write an independent verdict. Any unexplained
    discrepancy is a no-go. Trading at size is a separate, later, explicit operator
    decision and is out of scope here.

## Polling Cadence

Both agents are turn-based and poll on scheduled wakeups; this is not millisecond
control.

* From Arm until the order is terminal: every 5 to 10 seconds.
* From terminal order until trade confirmation: every 10 to 15 seconds.
* While awaiting resolution and redeem: every 30 to 60 seconds.

Polymarket applies a roughly 250 ms taker delay on selected crypto markets, and a
pending delayed order cannot be canceled during that window; fill-or-kill bounds
this because an unfilled order is killed rather than left resting.

## Abort

Abort immediately on any of: geoblock or eligibility failure, a prior unconfirmed
fill, more than one intent or order, a non fill-or-kill order, a partial fill, an
unexpected open or resting order, a venue, local, or on-chain mismatch, a risk or
drawdown breach, a stale bot heartbeat or process crash, a trade reported failed, a
prolonged retrying state, an unauthorized control command, a config fingerprint
drift after GO, feed or clock freshness degrading between Preflight and Arm, a
control command that does not match the frozen ticket, more than one eligible or
ambiguous candidate market, or any ambiguous state.

Abort channels, any one of which is sufficient:

* The bot's own auto-halt (primary, real-time).
* Codex emergency KillSwitch or CancelAll.
* Claude Disarm, StopAfterFlat, or KillSwitch.
* Operator from the phone: the dashboard kill or cancel control, an SSH session
  running `buba-paint live-control ... kill_switch`, a direct Polymarket cancel, or
  as final containment revoking the API key and reducing the allowance.

## Failure Modes And Mitigations

* Duplicate order: durable one-order-per-intent idempotency plus the 5 USD open
  notional ceiling.
* Phantom or short fill: post-fill on-chain reconciliation against the cumulative
  expected position, auto-halt on mismatch, and a startup refusal while any fill is
  unconfirmed.
* Partial fill-or-kill: treated as a blocking anomaly, not a normal fill.
* Bot crash mid-verification: the durable pending marker plus the startup refusal
  to trade past an unconfirmed fill.
* Wrong market or side: the frozen envelope and Codex verification before Arm cover
  the market and envelope; the realized side is signal-determined and can only be
  verified after the order, so it is checked at on-chain reconcile, not before Arm.
* No-fill: terminal no-trade, never an automatic retry.
* Settlement or redeem delay: bounded polling, RedeemAll only after resolution, and
  no further action until reconciled.
* Operator absent: every protection above is structural, not reaction-based, so the
  worst case stays about 5 USD without any human reacting fast.

## After The Canary

* Record the evidence bundle (preflight, fill, on-chain reconcile, settle, redeem,
  fee) under an ignored data path, not the repo root.
* Update [LIVE_READINESS_PLAN.md](./LIVE_READINESS_PLAN.md) Phase 6 with the outcome
  and present the written go-or-no-go for sizing to the operator.
