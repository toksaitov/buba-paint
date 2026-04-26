# Live Readiness Review

This document records the current quality bar of the live-readiness branch before any pass that removes the remaining write-path stubs or enables real venue execution.

## What exists now

The local tree already has:

- explicit execution modes: `paper`, `live_readonly`, `live_trading`
- additive live ledger tables and DB readers
- live agent endpoints and dashboard pages
- a local TypeScript sidecar for the authenticated Polymarket boundary
- `live-preflight` CLI wiring from the Rust bot into the sidecar
- a real `live_readonly` runtime in `buba-paint live`
- dynamic fee and venue metadata persistence for live-readiness and parity work

## What is intentionally stubbed

These boundaries are still intentionally non-live:

- `EXECUTION_MODE=live_trading` is still rejected by `buba-paint live`
- sidecar order placement, cancel, and redemption endpoints are still explicit not-implemented stubs
- the readonly runtime does not place real orders or populate live order/fill/redemption tables from local execution
- redemption submission and live order or fill ingestion are not yet wired to Polymarket

This means `live-preflight` and `live_readonly` are real venue-readiness surfaces, but not a green light for live money yet. `live_readonly` now reuses the shared paper loop so the shadow analysis pages remain useful as a shadow-performance view.

## What was reviewed in this pass

This review pass verified:

- execution-mode semantics are stated consistently across bot, sidecar, agent, dashboard, and docs
- live-mode config validation rejects malformed URLs and invalid small-bankroll envelopes
- the readonly runtime creates real live sessions, account snapshots, and reconciliation events without placing orders while continuing to populate the paper tables as a shadow track
- readonly session degradation follows the latest account snapshot instead of getting stuck on stale startup-preflight cash or allowance state
- the sidecar health, account, and preflight routes use real readonly-safe venue checks
- the sidecar process now has explicit websocket lifecycle, shutdown, timeout, and readiness behavior instead of relying on ad hoc cleanup
- sidecar server request validation rejects malformed JSON and malformed order/preflight bodies with `400`
- live DB tables, agent endpoints, and dashboard Execution page handle readonly and empty states cleanly
- docs and `.env.example` describe the `POLY_PROXY` account model and the current readonly-only state without stale claims

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

- real order lifecycle and redemption wiring
- user-stream-driven order and fill ingestion
- redemption and relayer integration
- live-trading runtime implementation and deployment soak

The current hardening pass does not deploy supervision automatically, but the repository now carries the intended supervised sidecar process model and service artifact for `buba-paint`.

See also [Next Pass: Real `live_trading`](./live-trading-next-pass.md) for the compact handoff note that should be used to recover context before starting that work.
