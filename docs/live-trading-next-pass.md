# Next Pass: Real `live_trading`

This note exists to preserve context after the `live_readonly` soak.

## Current state

- `paper` works and remains the baseline research mode.
- `live_readonly` works as intended:
  - shared runtime
  - shadow paper signals, trades, and stats
  - real Polymarket account monitoring
  - no real order placement
- `live_trading` is still intentionally gated.

## What the soak has already proven

- proxy-wallet auth and sidecar readonly integration work on the server
- real venue/account health can be monitored safely
- the old dashboard pages remain useful through the shadow-paper track
- the Execution page remains the real venue/account diagnostics surface
- the rejection telemetry is meaningful enough to explain inactive periods

## What still blocks real money

- real order submission
- real order-state ingestion and reconciliation
- cancel-all and disarm behavior
- restart recovery with open live state
- redemption handling
- key rotation before any funded live deployment

## Exact next implementation scope

The next code pass should implement the minimum safe `live_trading` slice:

1. Real order placement in the sidecar.
2. Real order and fill ingestion into the live tables.
3. Live order reconciliation against venue truth.
4. Cancel-all, disarm, and kill-switch behavior.
5. Restart recovery when live orders or positions already exist.

Do not broaden that pass into a full UI redesign or multi-strategy rollout.

## First live-money policy

The first real-money canary should stay narrow:

- `latency-arb` only
- taker-only
- tiny bankroll
- strict hard caps
- explicit operator arming
- immediate cancel-all / disarm path

`calm-persistence` and `spread-capture` stay disabled for the first live-money run even if they remain available in paper and readonly.

## UI and UX work that can proceed in parallel

UI work is reasonable now, but keep it operational:

- make mode/state easier to understand
- keep the old pages clearly labeled as shadow-paper output in `live_readonly`
- keep the Execution page clearly labeled as real venue/account truth
- improve readonly and future live control/status clarity

Do not design destructive live controls until the real trading path exists end to end.

## Operational notes to carry forward

- The readonly soak is healthy, but long runs grow SQLite materially because of feed capture.
- Before long future soaks or live-money sessions, tighten storage policy or add pruning/export discipline.
- The Polymarket private key and relayer API key were exposed during development and must be rotated before any real-money use.

## Ready-to-start checklist for the next pass

- readonly soak reviewed and accepted
- storage-growth decision made
- live wallet and relayer credentials rotated
- next pass scoped to the minimum live-trading path above
- full tests, docs, review, and gates required again before any funded deployment
