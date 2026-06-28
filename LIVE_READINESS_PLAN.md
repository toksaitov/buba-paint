# Live Readiness Plan

## Purpose And Status

This is the fresh explicit live-money readiness plan that `CLAUDE.md` requires
before arming real-money trading. It takes the bot from the current safe posture
(`paper` and Docker/Caddy `live_readonly`) to a state where a first real-money
order can be sent with known, mitigated failure modes.

It was built on 2026-06-27 from a dual investigation: a web review of Polymarket
venue changes over the prior three months and an independent read-only Codex
audit of the live-money code path, then reviewed by a Codex plus architect plus
completeness panel. The executable truth lives in the files cited per phase;
verify every file:line against current code before acting, since line numbers
drift.

Headline from that investigation: the Polymarket CLOB V2 plus pUSD hard cutover
of 2026-04-28 (new exchange contracts, rewritten order struct, USDC.e to pUSD
collateral, fees at match time) is already absorbed in our code. The sidecar is
on the post-cutover `@polymarket/clob-client-v2` 1.0.3, signs the V2 order struct
through the SDK, holds collateral in pUSD, and reads fees live per market. The
remaining work is a bounded set of safety, idempotency, and reconciliation gaps
plus mandatory empirical confirmations and a canary.

Operating posture stays unchanged until this plan completes and the operator
gives an explicit go: `paint` and `sidecar` remain stopped, and no real order is
sent except the single human-gated canary in Phase 6.

## How To Run This Plan

Run it with `/goal` in ultracode mode, one phase at a time, in order. Every code
change is designed and implemented together with Codex, and every phase is
reviewed by Codex before it is committed. The short kickoff prompt is at the end
of this document under "Kickoff Goal Prompt".

`$CODEX` below means the Codex companion script at
`~/.claude/plugins/cache/openai-codex/codex/<version>/scripts/codex-companion.mjs`.
Resolve `<version>` at runtime (the newest directory there).

## Preconditions (Before Phase 0)

These must all pass before any phase begins. They are one-time gates.

* Clean working tree on a dedicated working branch, not `master`. Create or
  switch to a branch first, because the numeric-change guard compares against
  `origin/master` and commits accumulate on the branch until the operator asks
  to push.
* `git fetch origin` so `origin/master` is current. The numeric-change guard
  `git diff --name-only origin/master -- bots/paint/src/strategies bots/paint/src/decision`
  is only meaningful against an up-to-date remote ref.
* Codex available: `node "$CODEX" setup --json` reports `ready: true` and
  `loggedIn: true`. Every phase mandates a Codex review, so if Codex is not ready
  or not logged in, STOP and surface to the operator (human gate). Do not proceed
  without Codex.
* Decide the Codex review-gate setting once and record it. To avoid a surprise
  stop-time double review, keep the harness review gate disabled
  (`node "$CODEX" setup --disable-review-gate`) so the per-phase adversarial
  review in this plan is the single Codex review mechanism. If the operator wants
  the stop-time gate enabled instead, treat that gate as the phase review and do
  not run a second one.

## Operating Rules (Hard Guardrails)

These bind every phase. A phase is not done if any rule is violated.

* `paint` and `sidecar` stay stopped. "Stopped" means the armed live trading
  runtime (the bot running in `live_trading`) is not running. Explicitly scoped
  read-only sidecar calls (health, account, activity, market metadata, geoblock)
  and the single Phase 6 canary are permitted; nothing else sends a real order.
* Do not change trading strategy, decision, or backtest-replay numerics. The
  guard `git diff --name-only origin/master -- bots/paint/src/strategies bots/paint/src/decision`
  must stay empty for the whole plan. This guard is scoped to those two
  directories only.
* Numeric defaults outside those two directories are still protected by policy.
  Any edit that changes live edge, sizing, or the backtest fee model, notably
  `bots/paint/src/config.rs` `taker_fee_rate` and `taker_fee_exponent` and the
  math in `bots/paint/src/fees.rs`, is numeric-sensitive: it must show up in the
  diff and requires explicit operator sign-off, and it is excluded from
  autonomous execution. Phases that touch these files for non-numeric reasons
  must leave the numeric constants unchanged.
* Honor the hot-path rules in `CLAUDE.md`. New venue input or output (on-chain
  reads, reconciliation, settlement resolution, account refresh, redemption,
  WebSocket watchdog) belongs in bounded workers, never the feed hot path. Any
  Rust-touching phase runs `make hot-path-audit` as a gate; Phase 3 and Phase 5
  must run it because they add venue I/O.
* Honor the engineering rules in `CLAUDE.md`: no `unwrap()` or `expect()` in
  library code, `f64` for money and probabilities, immutable `Config`, the
  `Clock` trait where time affects behavior, SQL only under `bots/paint/src/db/`
  or the owning boundary, rustdoc on every Rust function including tests and
  private helpers, and the TypeScript comment policy for the sidecar.
* Gates are green before any commit. The per-phase subset is at least `make lint`,
  the matching test target, `cargo clippy --workspace -- -D warnings`,
  `cargo fmt --all --check`, `make hot-path-audit` for Rust-touching phases, and
  for sidecar changes `cd polymarket-sidecar && npm test && npm run build`. The
  whole plan ends with `make test-all` and `cargo build --release` green.
* One focused commit per change, imperative mood. No AI, Codex, Claude, or
  Anthropic attribution anywhere in commits, messages, branches, or any other
  git or GitHub artifact. No `Co-Authored-By`.
* Push only when the operator asks.
* Update the relevant docs after any behavior, schema, API, deployment, or
  workflow change.
* Operator and empirical gates are human decision points. Autonomous execution
  stops and surfaces them rather than guessing. These are: the funded wallet
  signature type, the operating jurisdiction and egress IP plus the no-VPN
  confirmation, the funded pUSD balance, the Phase 6 canary go or no-go, and any
  numeric-sensitive change.

## Per-Phase Execution Protocol

Every phase runs this loop. In ultracode mode steps 2 through 7 are one workflow:
an implement stage, then a Codex-review stage, then a bounded fix loop.

1. Record the phase base commit: `BASE=$(git rev-parse HEAD)`. Pass it to the
   review in step 5.
2. Design with Codex. Claude proposes the approach and the exact files to touch,
   then consults Codex with `node "$CODEX" task --effort high "<design question>"`
   to critique the design before coding. If that call backgrounds instead of
   returning, poll per the Codex availability rules below. Reconcile the two
   views into a final approach.
3. Implement with Codex. Claude implements, keeping the diff scoped to this phase
   only, and pulls Codex in on any nontrivial or risky edit before finalizing it.
4. Self-gate. Run the per-phase gate subset locally and make it green.
5. Codex phase review. Run
   `node "$CODEX" adversarial-review --wait --base "$BASE" --scope branch "<phase Codex review focus>"`,
   passing this phase's "Codex review focus" string as the focus argument so the
   review is targeted.
6. Act on the review by severity:
   * Fix in-phase, autonomously: correctness, safety, hot-path, numeric-drift,
     idempotency, and test-adequacy findings.
   * Fix if cheap: cosmetic and style findings.
   * Escalate to the operator: genuinely out-of-scope findings and any
     numeric-sensitive change (strategy, decision, or fee model). Record the
     finding and the proposed handling; do not silently dismiss it.
   Re-run the Codex review until it returns no blocking findings, up to a maximum
   of three iterations. If blocking findings persist after three iterations, STOP
   and surface to the operator rather than looping forever.
7. Re-gate. Run the full per-phase gate subset again and make it green.
8. Commit. One focused commit per change, no attribution.
9. Record phase completion in the Phase Checklist and proceed.

Codex availability and stuck handling, used in steps 2 and 5:

* Poll a running Codex job with `node "$CODEX" status <job-id>`; fetch a finished
  result with `node "$CODEX" result <job-id>`; cancel with
  `node "$CODEX" cancel <job-id>`.
* Treat a job as stuck when the latest progress command is unchanged across two
  consecutive status checks about sixty seconds apart, or when elapsed exceeds
  about fifteen minutes with no new command. On stuck: cancel the job, retry once
  with a tighter prompt, and if it stalls again STOP and surface to the operator.
* If `node "$CODEX" setup --json` ever reports not ready or not logged in, STOP
  and surface to the operator. Never silently skip the Codex review step.

## Phases

### Phase 0 - Facts, Guards, And Venue Assumptions

Goal: lock down the facts that decide whether anything else is safe, add startup
and arming guards for them, and write a venue-assumptions checklist artifact.
This phase changes guards and docs only and sends no orders.

Items:

* Signature-type guard (blocker support). The sidecar defaults to
  `POLYMARKET_SIGNATURE_TYPE=1` (POLY_PROXY) in
  `polymarket-sidecar/src/config.ts:87` and branches on all four types in
  `polymarket-sidecar/src/provider.ts:342`. Add an operator-declared expected
  signature type and refuse to arm if the configured type does not match, so a
  fresh deposit wallet (type 3, POLY_1271) cannot be armed by accident. Open
  upstream bugs make type-3 order placement, API-key creation, and redemption
  fail.
* Jurisdiction and geoblock guard (blocker support). The sidecar has a
  `geoblock_check` stage but the bot currently treats geoblock as a soft issue,
  not a hard arming failure (verify around `bots/paint/src/live.rs:4389`). The
  mechanized half of this control is: make a blocked geoblock result a hard
  arming failure, and assert a recorded expected egress IP. The human half is the
  operator confirming the operating jurisdiction and that no VPN is in use; that
  half cannot be enforced in code and is an operator gate.
* Live venue-assumptions probe (nice-to-have artifact). Read-only, against a real
  `btc-updown-5m` market through the sidecar metadata path
  (`polymarket-sidecar/src/provider.ts:1545` `getClobMarketInfo`): capture live
  `fd.r`, `fd.e`, `fd.to`, min order size, and tick size. Write a checklist
  artifact at an ignored path (`data/live-readiness/venue-assumptions.json` if
  that path is gitignored, otherwise `/tmp/buba-live-readiness/venue-assumptions.json`;
  confirm the ignore status first, per the Data Preservation rule). Fields:
  capture timestamp, market slug, fee rate, fee exponent, taker-only flag, min
  order size, tick size, collateral token (pUSD address), configured signature
  type, CLOB/Gamma/WS hosts, and exchange EIP-712 domain version. Re-check before
  every funded run.

Acceptance: arming is refused when signature type mismatches the declared
expected type or when geoblock reports blocked. The assumptions artifact exists
at the resolved ignored path with all fields populated.

Operator gate: record the confirmed funded wallet signature type, the operating
jurisdiction and egress IP, and the no-VPN confirmation. Autonomous execution
stops here for these facts.

Codex review focus: guards fail closed, no guard sits on the feed hot path, the
probe is read-only, and the artifact path is genuinely ignored.

Tests: sidecar and bot tests for signature-type mismatch refusal and geoblock
hard-fail on arming.

Done when: guards merged and green, artifact written, operator facts recorded.

### Phase 1 - Risk-Cap Enforcement

Goal: make every named live cap a hard, enforced control on the live path.

Items:

* Enforce `live_max_open_notional_usd` on the submission path (blocker). It is
  configured (`bots/paint/src/config.rs:415`) and sent to preflight
  (`bots/paint/src/live_sidecar.rs:343`) but is not enforced at submission beyond
  generic bankroll and open-position controls. Add a hard pre-submit check near
  the single-order cap enforcement (`bots/paint/src/live.rs:4518`) that rejects an
  order when projected open notional would exceed the cap, including the
  multi-position case.
* Make `live_session_cash_cap_usd` a continuous cap (blocker). Today it is a
  startup clamp only (`bots/paint/src/live.rs:4343`, `:4301`). Add continuous
  enforcement so realized spend or equity cannot exceed the configured cap after
  startup, including after a deposit or venue-state drift.

Acceptance: an order that would breach open notional or the session cash cap is
rejected pre-submit with a clear rejection reason and a released reservation,
proven with a multi-position test.

Codex review focus: correctness of the projected-notional math, no double
counting against bankroll reservations, no numeric change to strategy or decision
code, and fail-closed rejection.

Tests: live-path tests asserting hard rejection for both caps as submission
blockers, including the multi-position case the current tests miss.

Done when: both caps enforced and tested, gates green, committed.

### Phase 2 - Order Idempotency And Timeout Safety

Goal: make a real order impossible to duplicate and make a local timeout unable
to become silent venue exposure. This phase is safe by blocking; Phase 3 later
relaxes blocking into reconcile-then-retry. Do not attempt reconciliation here.

Items:

* Durable idempotency (blocker, plus the nice-to-have schema uniqueness). The
  sidecar dedupes by `client_order_id` in process memory only
  (`polymarket-sidecar/src/provider.ts:2124`) and the DB has no uniqueness on
  `client_order_id` or `intent_id` (`bots/paint/src/db/schema.rs:343`). Add a
  durable uniqueness constraint and a submit path that treats a duplicate as a
  safe no-op, surviving a sidecar or bot restart. The migration must be forward
  safe: `client_order_id` is nullable and historical runtime DBs may contain NULL
  or duplicate values, so use a partial unique index where the id is not null, or
  a new clean-constrained column, and confirm an existing runtime DB still opens.
  Keep all SQL under `bots/paint/src/db/`.
* Block, do not retry, on timeout or unknown submission (blocker). The sidecar
  timeout wrapper does not abort the underlying SDK call
  (`polymarket-sidecar/src/provider.ts:1059`), so a local timeout can leave a real
  order in flight. Make the timeout abort the in-flight submission where possible.
  On the bot side, a timed-out, unknown, or partial-FOK submission must move to
  the existing blocking state and stop all further live submission, with no retry.
  The blocking plumbing already exists: `live_order_response_is_blocking`
  (`bots/paint/src/live.rs:759`) returns blocking for timeout, unknown_submission,
  venue_restart, and pending_unknown (`bots/paint/src/live.rs:4540`). Phase 3
  upgrades this to reconcile-then-retry once on-chain reconciliation exists; until
  then, block.
* Treat unexpected FOK partial fills as suspicious (should-fix). The bot accepts
  any accepted size in range as a legitimate partial (`bots/paint/src/live.rs:811`)
  while it only sends FOK (`bots/paint/src/live.rs:697`). A FOK should fill fully
  or not at all; treat a partial FOK as an anomaly that blocks and awaits
  reconciliation rather than a normal fill.

Acceptance: a retried or restarted submission cannot create a second real order
and the migration does not break an existing runtime DB; a timed-out or unknown
submission moves to the blocking state and is not retried; a partial FOK moves to
the blocking state.

Codex review focus: idempotency is genuinely durable across restart and the
migration is forward safe; the timeout path cannot both time out locally and
silently fill; the partial-FOK and timeout handling is fail-closed and does not
depend on reconciliation.

Tests: durable-idempotency-across-restart test, forward-safe migration test on a
pre-existing DB, timeout-then-late-success blocks-not-retries test, partial-FOK
anomaly test.

Done when: all three merged and tested, gates green, committed.

### Phase 3 - Settlement Reconciliation, Redemption, And Restart Windows

Goal: make fill truth match on-chain truth, make the redemption code path correct
for the current relayer, and make the recurring post-only restart windows safe.

Items:

* On-chain settlement reconciliation (blocker). There is no on-chain CTF
  `balanceOf` check anywhere; the bot reconciles via the CLOB activity feed
  (`bots/paint/src/live_sidecar.rs:218`). Polymarket has an open, unfixed bug
  where a matched market BUY can report success while on-chain balance stays zero,
  reportedly biased against winning fills (clob-client-v2 issue 54,
  py-clob-client issue 338). Add a bounded-worker reconciliation that reads the
  CTF balance (CTF address at `polymarket-sidecar/src/provider.ts:55`) and treats
  the on-chain position as the source of truth before a fill is counted as real.
  This reconciliation also upgrades the Phase 2 timeout-and-unknown handling from
  block-only to reconcile-then-retry: a blocked unknown submission may be retried
  only after on-chain reconciliation proves it did not fill. Keep all of this in a
  bounded worker, off the feed hot path.
* Redemption code path correctness (should-fix). The relayer `POST /submit` no
  longer returns a transaction hash; the flow must poll `GET /transaction`
  (`polymarket-sidecar/src/provider.ts:703`, `:2349`). Scope this phase to the
  code-verifiable half: confirm the proxy or Safe is actually deployed on-chain
  before relying on redemption, and implement and test the missing-hash poll
  handling. A full live end-to-end redeem requires a real settled position, which
  does not exist until the Phase 6 canary, so defer the live end-to-end redeem to
  post-canary.
* Confirm post-only and 425 handling pauses taking (should-fix). The sidecar
  detects 425 as a blocking `venue_restart` status
  (`polymarket-sidecar/src/provider.ts:998`, `bots/paint/src/live.rs:4539`).
  Confirm that during the roughly weekly matching-engine restart and its short
  post-only or cancel-only window the bot pauses taking and backs off rather than
  retrying rejected FOK or FAK orders.

Acceptance: a fill is only counted after on-chain confirmation; a blocked unknown
submission is only retried after reconciliation; the missing-hash relayer poll is
implemented and tested and the proxy/Safe deployment check works; a simulated
restart or 425 pauses taking with backoff.

Codex review focus: reconciliation lives in a bounded worker and not the hot
path; the on-chain read is authoritative over the CLOB ack; the reconcile-then-
retry upgrade cannot retry a submission that actually filled; the redemption poll
handles the missing-hash response.

Tests: on-chain reconcile mismatch test (phantom fill simulated), reconcile-then-
retry safety test, redemption hash-poll test, proxy/Safe deployment-check test,
restart and 425 pause-and-backoff test.

Done when: all three merged and tested, `make hot-path-audit` green, gates green,
committed.

### Phase 4 - SDK Currency And Venue Assumptions

Goal: get the client libraries current and the venue assumptions correct, without
touching numerics.

Items:

* Bump `@polymarket/clob-client-v2` 1.0.3 to 1.0.6 (should-fix). 1.0.5 carries a
  base64 decode fix that matters in Node; 1.0.4 and 1.0.6 are fixes and build
  tooling with no API change. Optionally bump
  `@polymarket/builder-relayer-client` 0.0.9 to 0.0.10. Re-run all sidecar gates
  and re-validate FOK and FAK dollar and share sizing after the bump.
* Live-fee assumptions check, numeric-sensitive and operator-gated (should-fix).
  Official docs show a crypto taker rate of 0.07. The preflight check that reads
  live `fd.r` and `fd.e` and logs a mismatch against the configured constant is
  implemented. With explicit operator approval the bot `taker_fee_rate` constant
  (and the `crypto_fee_params` post-changeover rate) were updated from 0.072 to
  0.07; the runtime still reads live per-market fees at match time. The edge and
  decision formulas were not changed, only the fee-rate constant.
* Cap bulk cancels at 1000 order IDs per request (nice-to-have), matching the
  2026-06-15 venue limit, on the cancel-all path.
* Gamma market discovery hardening (nice-to-have). Pass `closed=true` explicitly
  where closed markets are needed (the default is now false), plan migration to
  keyset pagination, and do not trust the neg-risk Gamma filter
  (`bots/paint/src/market_discovery.rs`).
* Builder attribution cleanup (nice-to-have). The V2 model attaches a
  `builderCode` field on the order; the HMAC builder-signing path may be dead
  weight for V2 order attribution, but it is not obviously dead:
  `builderRelayerClient` is destructured at `polymarket-sidecar/src/provider.ts:65`
  and `new BuilderConfig({...})` is constructed at
  `polymarket-sidecar/src/provider.ts:1583`. Decision procedure: trace whether the
  constructed `BuilderConfig` reaches the V2 order build or attribution. If it
  does not affect order attribution, remove it from the order path; if it does or
  is used for gasless deposit, withdraw, or redeem, keep it and document why.

Acceptance: sidecar on 1.0.6 with green gates and re-validated sizing; live-fee
mismatch is observable; cancel batches are capped at 1000; Gamma discovery is
robust to the closed default; the builder decision is made and documented.

Codex review focus: the SDK bump introduces no behavior change to order
construction; the fee work adds only a check and not a numeric change; Gamma
changes do not alter market-selection numerics.

Tests: sidecar order-construction tests on 1.0.6, fee-mismatch check test,
cancel-cap test, Gamma closed-default and filter tests.

Done when: all items merged and tested, gates green, committed; the fee constant
recommendation is recorded for the operator decision.

### Phase 5 - Observability And Resilience

Goal: close the remaining nice-to-have reliability gaps and add a safe dry-run.

Items:

* WebSocket staleness watchdog (nice-to-have). Guard against the silent-freeze
  failure where the socket stays open but delivers no data
  (`polymarket-sidecar/src/provider.ts:1085`). Add an inactivity watchdog that
  forces reconnect and a REST spot-check fallback, in a bounded worker.
* Clock-drift hardening (nice-to-have). The sidecar checks server-time drift and
  the bot treats drift as a terminal gate. Document NTP-sync as a host
  requirement and surface drift in health, since drift causes intermittent
  invalid-signature rejections.
* Disarmed would-submit dry-run (nice-to-have). Add a live-trading dry-run that
  exercises every live gate and builds the order intent but never calls the
  sidecar `/orders` endpoint, so the full armed path can be rehearsed without a
  real order. Deferred: this is the one open Phase 5 item. It adds a new branch to
  the live submission worker (the most safety-critical bot path) and is best built
  immediately before the canary as part of Phase 6 rehearsal, with operator input
  on exactly what to rehearse. It is a rehearsal convenience, not a safety blocker.
* ExchangeV3 watch (nice-to-have). ExchangeV3 (EIP-712 domain version 3) is merged
  into the client but not in production. Add a startup assertion of the expected
  exchange domain version and a note to watch the changelog, so a future V3
  cutover fails loudly rather than silently rejecting orders.

Acceptance: a frozen socket triggers reconnect; drift is visible in health; the
dry-run rehearses the armed path with no venue order; a domain-version mismatch
fails loudly at startup.

Codex review focus: the watchdog and dry-run do not touch the feed hot path or
strategy numerics, and the dry-run truly cannot send an order.

Tests: staleness-watchdog reconnect test, dry-run no-order test, domain-version
assertion test.

Done when: all items merged and tested, `make hot-path-audit` green, gates green,
committed.

### Phase 6 - Readiness Validation And Canary

Goal: prove the whole armed path on a single real order before any sizing, then
present a go or no-go.

Items:

* Full gate set: `make lint`, `make test-all`, `cargo build --release`,
  `cd dashboard/client && npm run build`,
  `cd polymarket-sidecar && npm test && npm run build`, then the live-readiness
  flow per `docs/testing-and-validation.md`:
  `make live-readiness-local`, then
  `buba-paint validate-live-fidelity --db-path <db> --start <t> --end <t> --output <path>`
  (a CLI subcommand, not a make target), then `make live-readiness-host-soak` for
  the no-order host soak.
* Operator gates. Confirm the funded wallet signature type, the operating
  jurisdiction and egress IP, the no-VPN status, and the funded pUSD balance.
* Canary. With explicit operator approval, send one minimum-size real order,
  reconcile it on-chain, and record the on-chain outcome (filled and confirmed
  versus phantom or reverted) and the realized fee against the live `fd.r`. A
  single order cannot produce a meaningful revert rate; a measured rate requires
  a later sized-trading phase, which is out of scope for this plan. This is the
  only real order in the plan. With a real settled position now in hand, exercise
  the deferred live end-to-end redeem from Phase 3.
* Reconciliation residuals to resolve before sustained unsupervised trading
  (Phase 3 review findings, accepted for the supervised single-order canary):
  durable startup re-drive of on-chain verification for fills still inside the
  post-fill verify window when the process stopped or crashed (today a crash inside
  that window loses detection silently); and the cumulative-balance masking case
  where two strategies fill the same leg in one window, which the absolute
  `balanceOf >= expected` check cannot isolate without a hot-path pre-fill snapshot.
  Both are documented in `bots/paint/src/live.rs`; the canary is a single
  operator-watched order, so neither bites until sustained trading.
* Codex final review of the full set of changes since the plan started, for
  arming readiness.

Acceptance: all gates green, operator facts confirmed, canary order placed and
reconciled on-chain with its on-chain outcome and realized fee recorded, the live
redeem exercised, and Codex final review clean.

Codex review focus: end-to-end arming safety, that all prior phase findings were
actually fixed, and whether the canary evidence supports or blocks scaling.

Tests: the full repository test suite plus the live-readiness flow.

Done when: canary verified and a written go or no-go is presented to the operator.
Arming at size is a separate explicit operator decision and is out of scope for
autonomous execution.

## Definition Of Done

* Every blocker, should-fix, and nice-to-have item above is resolved or has an
  explicit operator-approved deferral.
* `git diff --name-only origin/master -- bots/paint/src/strategies bots/paint/src/decision`
  is empty, and no numeric-sensitive constant changed without operator sign-off.
* Each phase has a Codex review with no outstanding blocking findings (or an
  operator-approved escalation), and `make hot-path-audit` is green on every
  Rust-touching phase.
* `make test-all` and `cargo build --release` are green.
* The canary order is placed and reconciled on-chain, with its on-chain outcome
  and realized fee recorded, and the live redeem exercised.
* A final go or no-go for arming at size is presented to the operator.

## Phase Checklist

* [x] Preconditions (branch, fetch, Codex ready, review-gate setting)
* [x] Phase 0 - Facts, guards, and venue assumptions
* [x] Phase 1 - Risk-cap enforcement
* [x] Phase 2 - Order idempotency and timeout safety
* [x] Phase 3 - Settlement reconciliation, redemption, and restart windows
* [x] Phase 4 - SDK currency and venue assumptions
* [ ] Phase 5 - Observability and resilience
* [ ] Phase 6 - Readiness validation and canary

## Kickoff Goal Prompt

Paste this to start, with `/goal` and ultracode enabled:

Resolve every item in LIVE_READINESS_PLAN.md, blockers then should-fix then
nice-to-have, one phase at a time in order, in ultracode mode. First satisfy the
plan Preconditions (work on a branch, fetch origin, confirm Codex is ready, set
the review-gate). Then follow the Per-Phase Execution Protocol exactly: record the
phase base sha, design and implement every change together with Codex, run a Codex
adversarial review of the full phase diff with the phase focus, and fix everything
it flags by the severity rubric before gating and committing, with a three-
iteration cap before escalating to me. Obey the Operating Rules: paint and sidecar
stay stopped, the strategies and decision directories stay byte-identical to
origin/master, numeric-sensitive constants need my sign-off, new venue I/O stays
in bounded workers with make hot-path-audit green, no real order except the Phase
6 canary, green gates per phase, one focused commit per change with zero AI or
Codex attribution, and push only when I ask. If Codex gets stuck or unavailable,
follow the plan recovery and stop for me rather than skipping a review. Stop and
ask me only at the operator gates: funded wallet signature type, operating
jurisdiction and egress IP and no-VPN, funded pUSD balance, the Phase 6 canary go
or no-go, and any numeric-sensitive change.
