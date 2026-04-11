# Live Readiness Review

This document records the current quality bar of the live-readiness branch before any pass that removes stubs or enables real venue execution.

## What exists now

The local tree already has:

- explicit execution modes: `paper`, `live_readonly`, `live_trading`
- additive live ledger tables and DB readers
- live agent endpoints and dashboard pages
- a local TypeScript sidecar for the authenticated Polymarket boundary
- `live-preflight` CLI wiring from the Rust bot into the sidecar
- dynamic fee and venue metadata persistence for live-readiness and parity work

## What is intentionally stubbed

These boundaries are still intentionally non-live:

- `buba-paint live` refuses `EXECUTION_MODE=live_readonly` and `EXECUTION_MODE=live_trading`
- the sidecar provider is a stub and does not place orders
- the sidecar does not yet verify real geoblock, allowance, user-stream, or remote cash state
- redemption submission and user-stream-driven reconciliation are not yet wired to Polymarket

This means `live-preflight` is currently a contract-validation surface, not a venue-readiness green light.

## What was reviewed in this pass

This review pass verified:

- execution-mode semantics are stated consistently across bot, sidecar, agent, dashboard, and docs
- live-mode config validation rejects malformed URLs and invalid small-bankroll envelopes
- the sidecar contract fails explicitly and honestly in stub mode instead of implying venue readiness
- sidecar server request validation rejects malformed JSON and malformed order/preflight bodies with `400`
- live DB tables, agent endpoints, and dashboard Live page handle both seeded and empty states cleanly
- docs and `.env.example` describe the `POLY_PROXY` account model and the current stubbed state without stale claims

## Verified local gates

The current branch is expected to be clean under:

- `make lint`
- `make comment-audit`
- `make test-all`
- `make coverage-gate`
- `cargo build --release`
- `cd dashboard/client && npm run build`
- `cd polymarket-sidecar && npm run build`

## Next-pass boundary

The next pass may replace stubs with real authenticated venue wiring. It should not need to first clean up docs, test scaffolding, or contract ambiguity from this branch.

The main remaining work is:

- real sidecar provider implementation
- real venue preflight checks
- real order lifecycle and redemption wiring
- reconciliation against venue truth instead of seeded or stubbed state
