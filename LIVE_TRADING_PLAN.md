# Live Trading Plan

This is an active implementation plan. It belongs in the repository root because it is unfinished work. Do not move it into `docs/`. When the work is complete, delete this file and convert only durable system facts into stable docs.

## Operating Discipline

This work deals with real money. Every phase must follow [AGENTS.md](./AGENTS.md), which points to [CLAUDE.md](./CLAUDE.md) as the canonical repository instruction file.

Required behavior for every implementation pass:

- Be thorough. Read the relevant code and docs before changing behavior.
- Think hard about failure modes, not only happy paths.
- Iterate until the phase is actually complete. Do not stop at partial wiring that looks plausible.
- Do not cut corners on tests, docs, or verification because the first bankroll is small.
- Verify Polymarket behavior against current official docs before implementing venue behavior.
- Verify venue assumptions from the actual deployment host before arming real money.
- Prefer fail-closed behavior for money, orders, fills, redemption, auth, geoblock, account state, and reconciliation.
- Treat unknown order state as dangerous until reconciled.
- Treat stale or missing data as a trading blocker, not as an empty or successful state.
- Keep secrets out of logs, DB rows, test fixtures, and docs.
- Use small, separately reviewable phases. Do not combine docs cleanup, SDK migration, order routing, dashboard controls, and deployment in one pass.
- Run the planned validation gates. If a gate is too slow or unavailable, record that explicitly and do not silently skip it.
- Resolve newly discovered issues before closing a phase. Do not leave correctness, safety, security, or contract problems as casual residual risk.
- Prefer system correctness over plan completion. If implementation reveals that the current phase plan is incomplete or wrong, update the plan and fix the discovered issue before advancing.

Research discipline for every phase:

- Re-read local durable docs that affect the phase.
- Search current official Polymarket docs for the venue surface being touched.
- Check the official changelog before touching sidecar, CLOB, relayer, user-stream, fee, collateral, or market metadata logic.
- When official docs conflict with existing local assumptions, treat local assumptions as stale until proven otherwise.
- Save durable findings in stable docs only after the implementation is complete. Keep active work in root while it is still active.

Issue-resolution discipline for every phase:

- Treat any failing validation gate, security audit finding at moderate severity or above, venue contract mismatch, secret-handling issue, stale account/order ambiguity, misleading UI state, or untested money path as blocking by default.
- A blocker must be fixed in the current phase or the phase must stop explicitly as blocked. Do not call the phase complete while the blocker remains.
- Low-severity upstream dependency advisories may be accepted only when there is no available patched version, the dependency is required by the official/current venue SDK, and the acceptance is documented in the phase result.
- If a new finding changes assumptions for later phases, edit this plan before continuing so future plan-mode runs inherit the corrected understanding.
- Every phase closeout must state: blockers found, blockers fixed, accepted low-risk debt if any, validation gates run, and gates intentionally skipped if any.
- Do not optimize for checking off all phases. A phase that discovers the plan is unsafe has succeeded if it prevents bad live-money behavior.

## Phase Status

- Phase 0, Documentation IA Reset: complete in commit `398da37` on 2026-05-01.
- Phase 1, Polymarket CLOB V2 Venue Contract Reset: complete in commit `6c2f2d5` on 2026-05-01.
- Phase 2, Sidecar Write Boundary: complete in commit `e8c223d` on 2026-05-01.
- Phase 3, Live Ledger and Bot Runtime: complete in commit `359393f` on 2026-05-02.
- Phase 4, Dashboard Execution Controls: complete in commit `53a1fe9` on 2026-05-02. Chosen defaults are admin-only dashboard controls and audited preflight commands queued through the bot control ledger.
- Phase 5, Risk, Halt, and Human Cooldown Policy: complete in commit `c406e39` on 2026-05-02. Chosen defaults are postmortem-only cooldown, new run DB after terminal halt, current loss caps, 2-minute terminal degradation threshold, and cancel/redeem still available from halted sessions.
- Phase 6, Replay-Grade Real-Money Capture: complete in commit `21e4d5d` on 2026-05-02. Chosen scope is public replay gate only; the deployment-host readonly soak is prepared but not run in this phase.
- Phase 7, Live Fidelity and Replay Explainability Gate: complete in current local work on 2026-05-02, pending commit. Host rollout and real-money canary remain gated until later phases are implemented and verified.

## Current Decision

The next major project direction is a small-bankroll real-money implementation. The first funded canary should use about `75-100 USD`, strict caps, explicit operator arming, and latency-arb only by runtime config.

Implement live-money support for all strategy families. Do not remove `spread-capture` or `calm-persistence` from the codebase. The first live-money run disables those families by config:

```bash
LATENCY_ARB_ENABLED=true
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
```

The live execution layer must still be architecture-ready for spread and calm. Spread is especially risky because its two legs are not atomic. It needs explicit residual-exposure handling before it can be enabled with real money.

## Current External Contract State

Polymarket CLOB V2 is now production. The sidecar readonly CLOB boundary uses:

- `@polymarket/clob-client-v2`
- `@polymarket/builder-relayer-client` plus `@polymarket/builder-signing-sdk` for gasless redemption submission

Official Polymarket docs say CLOB V2 went live on production on April 28, 2026, legacy V1 SDKs and V1-signed orders are no longer supported, pUSD replaced USDC.e as trading collateral, and V2 order signing removed fields such as `feeRateBps`, `nonce`, and `taker`.

This means live order routing cannot use any V1 order assumptions. The sidecar contract has been reset to CLOB V2, pUSD diagnostics, FOK/FAK market-order submission, cancellation, and gasless CTF redemption. Bot runtime arming and dashboard control enablement are still intentionally gated for later phases.

The installed `@polymarket/builder-relayer-client@0.0.8` TypeScript package exposes the constructor and auth shape used by the builder signing SDK. The sidecar therefore fails redemption closed unless `POLYMARKET_BUILDER_API_KEY`, `POLYMARKET_BUILDER_SECRET`, and `POLYMARKET_BUILDER_PASSPHRASE` are configured. Plain relayer API key fields remain recorded in config for observability, but they are not treated as sufficient for redemption with the installed SDK.

Official sources checked on 2026-05-01. Re-check them at the start of the relevant phase because these docs are live operational dependencies:

- https://docs.polymarket.com/changelog
- https://docs.polymarket.com/v2-migration
- https://docs.polymarket.com/api-reference/authentication
- https://docs.polymarket.com/trading/orders/overview
- https://docs.polymarket.com/trading/orders/cancel
- https://docs.polymarket.com/market-data/websocket/user-channel
- https://docs.polymarket.com/trading/fees
- https://docs.polymarket.com/trading/matching-engine
- https://docs.polymarket.com/concepts/pusd
- https://docs.polymarket.com/concepts/resolution
- https://docs.polymarket.com/trading/gasless
- https://docs.polymarket.com/trading/bridge/withdraw
- https://docs.polymarket.com/api-reference/introduction

Local Gamma probes from this development machine returned `403`. A no-order deployment-host check on `2026-05-01` passed the Polymarket geoblock endpoint but Gamma BTC 5-minute discovery returned HTTP `403` with body `error code: 1010`. Before arming real money, resolve host-safe market discovery and revalidate BTC 5-minute metadata from the actual deployment host.

## Non-Negotiable Safety Rules

- `live_trading` remains gated until all phases below pass.
- No live order path may serve stale or fabricated account facts.
- `/account` remains fail-closed for money, orders, fills, inventory, and allowance.
- The dashboard must never imply trading is armed when it is only running or readonly.
- Drawdown hard-stop is terminal for the session. Restart requires export, analysis, a cooldown, and a new explicit operator decision.
- Mobile UI may observe and disarm. It must not be the primary arm surface for the first release.
- `FEED_EVENT_STORAGE_PROFILE=replay_grade` is mandatory for funded runs unless a short forensic exception is explicitly documented.
- Every funded session must be reproducible enough to diagnose why each order was placed, filled, missed, canceled, redeemed, or blocked.
- The first canary must use latency-arb only by config, even though all strategies must be live-capable in code.

## Phase 0: Documentation IA Reset

Goal: separate stable system docs from active implementation work before adding more live-trading complexity.

Rules:

- `docs/` is stable system documentation only.
- Active plans stay in root and are deleted when finished.
- Run-specific analysis stays under `data/experiments/...`.
- Historical context goes under `docs/archive/` only when it explains past decisions.
- `Readme.md` must shrink into a project entrypoint that links to focused docs instead of trying to be the whole context.
- `CLAUDE.md` must stay compact enough for agents to load. It should contain repo rules and a module map, not long historical plans.

Tasks:

- Remove stale active handoff docs from `docs/` and keep this root file as the active source of work.
- Keep only durable live-trading facts in `docs/live-trading-architecture.md`, `docs/live-session-runbook.md`, and `docs/polymarket-live-constraints.md`.
- Add or update a docs map that clearly says which docs are current, archived, and historical.
- Split oversized `Readme.md` content into focused stable docs if needed.
- Update `scripts/audit-docs.py` so active-plan filenames are rejected under `docs/`.
- Keep `PLAN.md` forbidden, but allow specific root active plans such as `LIVE_TRADING_PLAN.md`.

Acceptance gates:

- `make docs-audit`
- `make comment-audit`
- `git diff --check`
- No active implementation plan remains under `docs/`.

## Phase 1: Venue Contract Reset for CLOB V2

Goal: make the sidecar understand the current production Polymarket contract before any live order implementation.

Tasks:

- Replace legacy CLOB dependencies with current V2 packages or official current equivalents.
- Remove V1 order assumptions from the sidecar:
  - no signed `feeRateBps`
  - no order nonce handling
  - no V1 taker field assumptions
  - no old builder signing flow for order attribution
- Add V2 pUSD collateral awareness:
  - account balance naming should not imply USDC.e if the venue uses pUSD
  - allowance checks must target the current collateral and exchange contracts
  - docs and UI copy must use the current collateral model
- Revalidate CLOB auth:
  - L1 key derivation
  - L2 headers
  - proxy-wallet signature type
  - funder/proxy wallet behavior
- Revalidate current BTC 5-minute market metadata from the deployment host:
  - `orderMinSize`
  - `orderPriceMinTickSize`
  - `feesEnabled`
  - fee details from `getClobMarketInfo`
  - token IDs
  - accepting-orders behavior before window start
- Revalidate geoblock from the deployment host.
- Revalidate matching-engine restart behavior and HTTP `425` handling.
- Revalidate official Data API and Bridge API surfaces needed for account, position, activity, redemption, and withdrawal observability.
- Decide explicitly whether any Bridge withdrawal automation belongs in v1. Default should be no. The first live bot should redeem winning positions inside Polymarket and leave external withdrawal as a manual or later-phase workflow unless separately planned and tested.

Tests:

- Sidecar auth bootstrap tests for the V2 client shape.
- Sidecar metadata tests for fee, tick size, min size, token IDs, and pUSD balance fields.
- Sidecar error classification tests for `425`, auth failure, geoblock failure, and unavailable CLOB endpoints.
- A readonly production smoke test path that does not place orders.

Acceptance gates:

- `cd polymarket-sidecar && npm test`
- `cd polymarket-sidecar && npm run build`
- `make lint`
- `make docs-audit`
- A deployment-host readonly preflight report saved under `data/experiments/venue-contract-v2-001/`

## Phase 2: Sidecar Write Boundary

Goal: implement the narrow authenticated venue boundary for real orders, cancellation, and redemption, without changing strategy logic yet.

Endpoints to make real:

- `POST /orders`
- `POST /cancel`
- `POST /cancel-all`
- `POST /redeem-all`

Order behavior:

- Use marketable FAK or FOK semantics deliberately, not accidental resting orders.
- Preserve a client order ID generated by the bot.
- Return venue order ID, status, accepted size, fill hints, rejection reason, fee metadata, and raw-safe details.
- Treat partial fill as a first-class result.
- Treat HTTP `425` as temporary venue restart. Pause submission and retry only under controlled policy.
- Treat unknown submission outcome as dangerous. Persist it and reconcile before submitting more risk.

Cancellation behavior:

- `cancel` cancels one known order.
- `cancel-all` cancels all open orders for the account.
- Return canceled IDs and not-canceled reasons.
- `cancel-all` must be available from disarm and kill-switch paths.

Redemption behavior:

- Identify redeemable resolved positions.
- Submit redemption through the current relayer or contract path.
- Track relayer transaction state until terminal.
- Do not count redemption proceeds as spendable until `/account` confirms the cash.

Data capture:

- Persist request and response summaries without storing private keys or secrets.
- Persist enough venue metadata to reproduce order economics:
  - condition ID
  - token ID
  - side
  - limit price
  - size or amount
  - order type
  - market tick size
  - market min size
  - fee details
  - CLOB server time if used
  - client receive and submit timestamps
  - venue status transitions

Tests:

- Mock V2 client order creation and posting.
- Mock cancel single, cancel all, and partial not-canceled responses.
- Mock redemption success, retrying, failed, and unknown transaction states.
- Verify no endpoint can return fake success.
- Verify secrets are never logged or persisted.
- Verify idempotent retries do not duplicate orders for the same client order ID.

Acceptance gates:

- `cd polymarket-sidecar && npm test`
- `cd polymarket-sidecar && npm run build`
- `cd polymarket-sidecar && npm run audit:security`
- sidecar manual dry-run against mocked provider
- no bot `live_trading` enablement yet

## Phase 3: Live Ledger and Bot Runtime

Goal: replace the current `live_trading` gate with a real armed runtime that shares the strategy decision engine but submits orders to the sidecar.

Runtime model:

- Start process in `live_trading` but disarmed.
- Run preflight.
- Require explicit arm command.
- Evaluate strategies through the existing shared strategy cycle.
- Convert accepted strategy intents into sidecar order intents.
- Persist every intent before sending it.
- Persist every venue response, fill, cancel, redemption, and reconciliation transition.
- On restart, rebuild live state from DB and venue truth before allowing arming.

Strategy support:

- Latency-arb places one marketable buy on the predicted side.
- Calm-persistence places one marketable buy on the selected side, but remains disabled for first canary config.
- Spread-capture places two independent marketable buys only when explicitly enabled. It must persist leg IDs separately and handle one-leg residual exposure. It must not claim atomic execution.

Initial canary policy:

```bash
EXECUTION_MODE=live_trading
LIVE_SESSION_CASH_CAP_USD=100
LIVE_MAX_SINGLE_ORDER_USD=10
LIVE_MAX_OPEN_NOTIONAL_USD=25
LIVE_MAX_DAILY_LOSS_USD=15
LIVE_MAX_SESSION_DRAWDOWN_USD=20
LIVE_MIN_REQUIRED_CASH_USD=25
LATENCY_ARB_ENABLED=true
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
```

Required state machines:

- live session lifecycle
- order intent lifecycle
- venue order lifecycle
- trade/fill lifecycle
- redemption lifecycle
- reconciliation lifecycle
- drawdown halt lifecycle

Reconciliation rules:

- User-stream is primary for fast updates.
- Polling account and open orders is the recovery truth.
- Any mismatch between local order state and venue truth becomes a reconciliation event.
- Critical reconciliation disables arming and may force cancel-all.
- Unknown order outcome blocks new orders until resolved or canceled.

Tests:

- Bot refuses to arm without healthy sidecar, geoblock, account, user stream, market metadata, and replay-grade capture.
- Bot places a latency order through a mocked sidecar and persists intent before call.
- Bot handles sidecar accepted, rejected, partial fill, unknown, and timeout responses.
- Bot recovers after restart with an open order.
- Bot disarms on reconciliation failure.
- Bot hard-stops on session drawdown.
- Bot keeps spread/calm disabled by config for the first canary, while their live paths remain covered by tests.

Acceptance gates:

- `cargo test -p buba-paint live`
- `cargo test -p buba-paint live_system`
- `cargo build --release -p buba-paint`
- mocked end-to-end live runtime test with sidecar stub

Phase 3 closeout:

- Implemented local disarmed `live_trading` startup, durable live-control command queue, live-control audit state, sidecar `/activity`, Rust activity recovery, live order intent persistence before sidecar submission, venue response/fill persistence, unknown-order blocking, spread residual critical reconciliation, and read-only dashboard summary states for `disarmed`, `armed`, `halted`, and `unknown_order`.
- Blockers found and fixed: stale agent/dashboard `live_trading` state still reported `gated`; sidecar lifecycle mocks missed the new activity contract; Clippy flagged long Phase 3 order-path functions; dashboard Vite/PostCSS dev dependencies had fixable audit advisories.
- Accepted low-risk debt: dashboard production bundle still emits the existing chunk-size warning above 500 kB.
- Gates run: `cargo test -p buba-paint live`, `cargo test -p buba-paint live_sidecar`, `cargo test -p buba-paint live_control`, `cargo test -p buba-paint --test live_system_test`, `cargo test -p buba-agent`, `cd polymarket-sidecar && npm test`, `cd polymarket-sidecar && npm run build`, `cd dashboard/client && npm test -- --run src/lib/__tests__/trading-summary.test.ts`, `cd dashboard/client && npm run build`, `cd dashboard/client && npm audit`, `make lint`, `make docs-audit`, `make comment-audit`, `cargo build --release -p buba-paint`, and `git diff --check`.
- Gates intentionally skipped: none from the Phase 3 plan. Full workspace `make test-all` and dashboard Playwright are deferred because they are broader than this phase gate set.

## Phase 4: Dashboard Execution Controls

Goal: turn the Execution page from a gated cockpit into a real operator surface without making dangerous actions easy.

Controls:

- Preflight
- Arm
- Disarm
- Cancel All
- Stop After Flat
- Redeem
- Kill Switch

Control rules:

- Process state is separate from trading state.
- `Running` never means armed.
- Arm requires a typed confirmation, current config fingerprint, cash cap preview, enabled strategies, geoblock status, user-stream status, and current market metadata.
- Disarm is always easier than arm.
- Kill switch must cancel all, disarm, and block re-arming for the session.
- Drawdown halt must be visually dominant and must not provide a quick restart button.
- Mobile may show observe, disarm, and kill-switch, but arming should be desktop-first for v1.

Execution page must show:

- current trading state
- enabled strategy set
- cash available and cash cap
- open venue orders
- pending fills or unknown outcomes
- live positions and redeemable inventory
- latest reconciliation status
- active hard-stop reason
- session audit log

Overview page must stay compact:

- process state
- shadow or live performance snapshot
- open exposure
- execution health summary
- recent outcomes

Tests:

- Paper mode does not show Polymarket `n/a` account walls.
- Live readonly shows account truth but no arm capability.
- Live trading disarmed shows preflight and arm state.
- Armed state is distinct from process running.
- Drawdown halted state hides restart shortcuts.
- Mobile cannot accidentally arm.

Acceptance gates:

- `cd dashboard/client && npm test`
- `cd dashboard/client && npm run build`
- `make test-e2e`

Phase 4 closeout:

- Implemented dashboard-to-agent live-control routing, agent command queueing, control audit reads, audited `preflight`, admin-only dashboard mutations, actor injection, typed confirmations, state-specific Execution controls, control audit rendering, halted/unknown-order dominant states, and mobile arm blocking.
- Blockers found and fixed: dashboard controls initially rendered every live action in every live-trading state; agent control write path needed stricter input/confirmation/session validation; dashboard control submission needed a fresh DB role check in addition to JWT role claims; bot preflight command application needed direct test coverage; E2E nav selectors were ambiguous after Overview shortcut links; Clippy flagged the agent control write path.
- Accepted low-risk debt: Vite still emits the existing chunk-size warning above 500 kB, Vitest/Playwright still print existing localstorage and `NO_COLOR` warnings, and Playwright keeps the existing intentional viewport skips.
- Gates run: `cargo test -p buba-agent live_control`, `cargo test -p buba-dashboard`, `cargo test -p buba-paint live_control`, `cargo test -p buba-paint live`, `cargo test -p buba-paint live_system`, `cd dashboard/client && npm test`, `cd dashboard/client && npm run build`, `make test-e2e`, `make lint`, `make docs-audit`, `make comment-audit`, `cargo build --release -p buba-paint`, and `git diff --check`.
- Gates intentionally skipped: none from the Phase 4 plan.

## Phase 5: Risk, Halt, and Human Cooldown Policy

Goal: prevent degenerate restart behavior and encode the operational discipline learned from the last run.

Hard stops:

- max session drawdown
- max daily loss
- reconciliation critical
- user-stream outage beyond threshold
- account refresh failure beyond threshold
- geoblock failure
- auth failure
- venue restart lasting too long
- unresolved unknown order outcome
- storage quality failure

Drawdown policy:

- Once `MAX_DRAWDOWN_PCT` or live session drawdown trips, the session is terminal.
- The bot stops placing new orders.
- The dashboard shows the halt reason, time, HWM, trough, current equity, and last orders.
- Restart requires a new run/session, exported data, a written analysis note, and a manual config fingerprint change or explicit no-change signoff.
- No one-click resume after a major drawdown.

Operator artifacts:

- final DB integrity check
- replay-data quality report
- account export
- order and fill export
- redemption report
- postmortem stub in `data/experiments/run-XXX-live-canary-NNN/`

Tests:

- Drawdown halt blocks orders immediately.
- Restarting `live_trading` over a halted or unknown-order DB fails fast. The next funded attempt must use a new run DB after closeout and postmortem.
- Dashboard shows halt reason without exposing a quick arm path.
- Control audit records who did what and when.

Acceptance gates:

- Rust live-system tests for halt persistence.
- Dashboard tests for halted-state UX.
- Runbook updated after implementation, not before.

Phase 5 closeout:

- Implemented live risk monitoring for session drawdown, daily loss, percentage drawdown, terminal gate failures, critical reconciliation, unknown submissions, and prolonged remote degradation.
- Implemented terminal halt behavior: block new submissions, attempt cancel-all, persist critical reconciliation/control-audit evidence, preserve halted or unknown-order state on shutdown, and reject `live_trading` restart against halted or unknown-order DBs.
- Implemented `live-closeout` export with DB integrity, replay-quality report, live ledger exports, control audit, summary, manifest, and postmortem stub.
- Exposed risk and closeout summary fields through the agent trading summary and dashboard Execution halted state.
- Blockers found: lint surfaced closeout helper quality issues, and concurrent validation exposed a `latency-arb` timestamp subtraction overflow when test/replay clocks moved backward.
- Blockers fixed: closeout helpers now pass lint without suppressions, and latency-arb cooldown/adaptive-threshold elapsed calculations use saturating arithmetic with regression tests.
- Accepted low-risk debt: dashboard production build still emits the existing Vite chunk-size warning; Playwright E2E still has the existing skipped viewport cases.
- Validation gates run: `cargo test -p buba-paint live -- --nocapture`, `cargo test -p buba-paint --test live_system_test -- --nocapture`, `cargo test -p buba-paint latency_arb -- --nocapture`, `cargo test -p buba-paint live_control -- --nocapture`, `cargo test -p buba-agent live -- --nocapture`, `cargo test -p buba-dashboard -- --nocapture`, `cd dashboard/client && npm test`, `cd dashboard/client && npm run build`, `cd polymarket-sidecar && npm test`, `cd polymarket-sidecar && npm run build`, `make test-e2e`, `make lint`, `make docs-audit`, `make comment-audit`, `cargo build --release -p buba-paint`, and `git diff --check`.
- Gates intentionally skipped: none from the Phase 5 plan.

## Phase 6: Replay-Grade Real-Money Capture

Goal: make every real-money run useful for later research, even if the strategy loses money.

Public market capture:

- `binance:aggTrade` with price, size, signed quantity, event time, and receive time.
- `binance:bookTicker` with best bid, best ask, bid size, ask size, event time, and receive time.
- `binance:depth` with depth notional or imbalance fields.
- `chainlink:chainlink_price`.
- CLOB UP and DOWN top-of-book with price, size, source timestamp if available, and local receive timestamp.
- market metadata at discovery and activation.

Private account capture:

- live order intents
- venue order responses
- user-stream order events
- user-stream trade events
- confirmed fills
- cancellations
- redemption transactions
- account snapshots
- reconciliation events
- control audit events

Storage policy:

- Keep public replay inputs in SQLite in compact typed form.
- Keep private payloads summarized and safe by default.
- Optional raw private forensic capture must be short, explicit, rotated, and stored outside SQLite.
- Do not put DB files in Git or LFS history again.

Validation:

- `validate-replay-data` must pass early in the session and at shutdown.
- A funded run is not accepted as research-grade until public capture passes the sweep-grade gate.
- If live private capture is incomplete, the run can still be useful, but it must be labeled honestly.

Tests:

- Live startup records replay-quality metadata.
- Storage profile cannot silently downgrade from replay-grade in `live_trading`.
- `validate-replay-data` is run as part of shutdown/export workflow.

Acceptance gates:

- one local replay-grade paper smoke DB passes `validate-replay-data`
- deployment-host readonly soak procedure is prepared but not run in this local-only phase
- no WAL or DB garbage in the repo root

Phase 6 closeout:

- Implemented observed replay-quality metadata semantics. `replay_quality_class` now reflects database evidence only, while configured capture capability is recorded separately.
- Implemented live-trading storage hardening: `FEED_EVENT_STORAGE_PROFILE=compact` fails fast in `live_trading`, and arming remains blocked until observed replay quality is `sweep_grade`.
- Reused the canonical `validate-replay-data` classifier for runtime metadata and closeout evidence instead of profile-implied or footprint-only approximations.
- Extended `live-closeout` summary, manifest, and postmortem stub with replay-quality class, missing required classes, validation interval, and descriptive-only labeling.
- Added deterministic replay-grade fixture coverage for complete data, missing Binance `bookTicker`, missing Binance `depth`, empty data, and path-based validation.
- Prepared the future no-order deployment-host readonly soak checklist under stable operations docs; it was not run in this local-only phase.
- Blockers found: lint caught `refresh_remote_state` exceeding the line-count limit after replay-gate wiring.
- Blockers fixed: replay-gate wiring was extracted into a helper without suppressing lint.
- Accepted low-risk debt: host readonly soak is intentionally deferred by Phase 6 scope, and full live/backtest fidelity proof remains Phase 7.
- Validation gates run: `cargo test -p buba-paint replay`, `cargo test -p buba-paint live`, `cargo test -p buba-paint live_control`, `cargo test -p buba-paint --test live_system_test`, `/tmp` replay-grade smoke DB through `./target/release/buba-paint validate-replay-data`, `make lint`, `make docs-audit`, `make comment-audit`, `cargo build --release -p buba-paint`, `git diff --check`, and repo-root DB/WAL/SHM check.
- Gates intentionally skipped: deployment-host readonly soak, because Phase 6 prepared it but did not run it.

## Phase 7: Live Fidelity and Replay Explainability Gate

Goal: prove that the data collected from real trading is most likely enough for as-close-as-possible replay and backtesting before relying on funded-run data for research decisions.

This phase is required because the previous long run produced useful descriptive evidence but was not replay-grade enough for trusted sweeps. Do not assume the new capture is adequate just because `validate-replay-data` passes. The question is whether we can reconstruct the live decision state, order eligibility, fill opportunity, and post-trade accounting closely enough to explain real outcomes.

Required review:

- Map every live strategy input to a persisted field:
  - market window and open price
  - Binance trade price, signed quantity, book imbalance, depth features, and event lag
  - CLOB UP and DOWN quote price, size, timestamp, local receipt time, and inter-leg skew
  - Chainlink settlement context
  - fee metadata, tick size, min size, and accepting-orders state
  - portfolio router regime and active strategy family
  - reserve state and open exposure state
- Map every live execution input to a persisted field:
  - client order ID
  - token ID
  - side
  - limit price or marketable amount
  - FAK or FOK selection
  - requested size or amount
  - user pUSD balance used for fee-aware sizing if applicable
  - order submission timestamp
  - venue response timestamp
  - user-stream event timestamps
  - fills, partial fills, cancels, unknown outcomes, and reconciled states
- Map every live accounting transition to a persisted field:
  - cash available
  - reserved cash
  - inventory mark value
  - redeemable value
  - pending redemption value
  - fees
  - realized PnL
  - total equity
- Identify what cannot be replayed perfectly:
  - exact exchange matching queue position
  - unavailable hidden liquidity
  - network path differences between live and replay
  - CLOB matching-engine internal timing
  - relayer transaction timing
- Label those limitations explicitly in docs and postmortems instead of pretending the replay is exact.

Parity probes:

- Run a short live-readonly or paper capture and replay it through the backtester. The reconstructed signal features must match live-recorded signal features within defined tolerances.
- Run a mocked live order capture and replay the same persisted data. The replay must reproduce whether the order would have been legal, marketable, and fillable under the recorded book state.
- After the first controlled production write smoke, compare replay against the observed venue outcome:
  - strategy decision
  - chosen side
  - requested size
  - legal order checks
  - expected marketability
  - observed fill or no-fill
  - fee estimate vs actual fee
  - cash and inventory transition
- If replay cannot explain a live order outcome, block parameter sweeps and write a data-gap note before continuing.

Data-quality gates:

- Existing `validate-replay-data` still gates public market completeness.
- Add `validate-live-fidelity` as a funded-run fidelity report that checks private live execution completeness.
- A funded run may be labeled `research_grade_live` only if public replay data and private order lifecycle data both pass.
- A funded run with incomplete private lifecycle data must be labeled `descriptive_only_live`.
- Parameter sweeps against real trading data are blocked unless the run is labeled `research_grade_live`.

Tests:

- Unit tests for the live-input-to-persisted-field mapping.
- Fixture test where live-style signal features and replayed signal features match.
- Fixture test where a recorded marketable order can be replayed against recorded CLOB book state.
- Fixture test where missing private order lifecycle data downgrades the run to `descriptive_only_live`.
- CLI or script test for the funded-run fidelity report.

Acceptance gates:

- one local fixture reaches `research_grade_live`
- one intentionally incomplete fixture is downgraded to `descriptive_only_live`
- docs explain remaining unavoidable replay limitations
- parameter sweep tooling refuses `descriptive_only_live` funded runs

Phase 7 closeout:

- Implemented `validate-live-fidelity` with `research_grade_live`, `descriptive_only_live`, and `no_live_trading` classes.
- Extended sweep gating so funded `live_trading` intervals require private live fidelity in addition to public `sweep_grade` replay data.
- Extended `live-closeout` with `live_fidelity.txt`, summary and manifest live-fidelity details, and descriptive-only postmortem labeling when private lifecycle evidence is incomplete.
- Extended sidecar `/activity` details with raw-safe user-stream and CLOB trade lifecycle fields needed for later fill/cancel explainability.
- Added fixtures for complete live lifecycle, missing fills, missing signal features, missing fee metadata, missing marketability evidence, critical reconciliation, no-live intervals, and path-based validation.
- Blockers found: initial live-fidelity fixture linkage used the SQLite row-count return value instead of the inserted signal ID.
- Blockers fixed: fixture linkage now uses `last_insert_rowid`, and targeted live-fidelity tests prove the signal/metrics join.
- Accepted low-risk debt: exact queue position, hidden liquidity, network-path differences, matching-engine internals, relayer timing, host soak, funded write smoke, and real arming remain intentionally out of Phase 7.
- Validation gates run: `cargo test -p buba-paint live_fidelity`, `cargo test -p buba-paint replay`, `cargo test -p buba-paint backtest`, `cargo test -p buba-paint live`, `cargo test -p buba-paint live_control`, `cd polymarket-sidecar && npm test`, `cd polymarket-sidecar && npm run build`, `make lint`, `make docs-audit`, `make comment-audit`, `cargo build --release -p buba-paint`, and `git diff --check`.
- Gates intentionally skipped: deployment-host readonly soak, funded write smoke, and real arming, because Phase 7 is local implementation only.

## Phase 8: Verification Ladder

Goal: avoid jumping from code changes directly to money.

Step 1: local mocked tests.

- Sidecar mocked CLOB V2 and relayer.
- Bot mocked sidecar.
- Dashboard mocked APIs.
- Full unit and integration coverage for happy and failure paths.

Step 2: local dry run.

- `EXECUTION_MODE=paper`.
- replay-grade capture.
- fake representative data.
- dashboard Execution reviewed.

Step 3: server readonly soak.

- `EXECUTION_MODE=live_readonly`.
- real sidecar readonly checks.
- no order placement.
- deployment-host geoblock check.
- current BTC metadata check.
- user-stream health check.

Step 4: production write smoke with guardrails.

- Use a tiny funded wallet.
- Place one controlled minimum legal order only if explicitly approved.
- Cancel if resting.
- Verify user-stream, open orders, trade history, account state, and dashboard.
- Redeem only after a resolved winning position exists.

Step 5: first canary.

- latency-arb only.
- `75-100 USD` cash cap.
- strict per-order and open-notional caps.
- short run duration.
- active operator monitoring.
- immediate stop on critical alerts.

Step 6: post-run review.

- stop cleanly.
- archive DB and logs.
- export account/order/fill/redemption facts.
- run data-quality gate.
- write postmortem.
- only then decide whether to continue, adjust, or stop.

## Phase 9: Deployment Plan

Goal: deploy only after the implementation and verification ladder prove the system is ready.

Deployment shape:

- sidecar supervised from `~/buba-paint-live/current/polymarket-sidecar`
- sidecar env at `~/buba-paint-live/config/sidecar.env`
- sidecar log at `~/buba-paint-live/logs/sidecar.log`
- bot, agent, and dashboard started in the documented order
- DB and logs under a fresh runtime dir
- no DBs committed to Git

Pre-deploy local gates:

```bash
make lint
make comment-audit
make docs-audit
make test-all
cargo build --release
cd polymarket-sidecar && npm test && npm run build
cd dashboard/client && npm test && npm run build
git diff --check
```

Remote pre-arm gates:

- `readlink -f ~/buba-paint-live/current` matches intended release.
- Sidecar `/health` is live and readiness fields are sane.
- Agent `/health` is healthy.
- Dashboard `/health` is healthy.
- DB quick check returns `ok`.
- No stale bot, agent, dashboard, or sidecar processes remain.
- Host geoblock passes.
- Current BTC 5-minute market metadata is captured.
- `live-preflight` passes.
- Execution page agrees with CLI preflight.
- Arm capability is disabled until explicit operator confirmation.

## Phase 10: Clean Finish

Goal: remove the active plan and preserve only durable facts.

When live trading v1 is implemented, tested, and either deployed or deliberately stopped:

- Delete `LIVE_TRADING_PLAN.md`.
- Move durable architecture facts into `docs/live-trading-architecture.md`.
- Move operator steps into `docs/live-session-runbook.md`.
- Move current venue constraints into `docs/polymarket-live-constraints.md`.
- Move run-specific findings into `data/experiments/...`.
- Update `Readme.md` to stay short.
- Update `CLAUDE.md` only with durable module and workflow facts.
- Run `make docs-audit`.

## Immediate Next Plan-Mode Slice

The next implementation slice should be Phase 0 only:

- make docs stable-only
- move or delete stale future-work docs
- shrink `Readme.md`
- update docs audit to reject active plans under `docs/`
- keep this root plan as the active unfinished work marker

Do not start CLOB V2 migration in the same slice. The docs reset should be committed separately so later live-money work has a clean context base.
