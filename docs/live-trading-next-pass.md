# Future Pass: Real `live_trading`

This note preserves context for the eventual real-money implementation. It is not the immediate next task. The immediate research step is to collect a new replay-grade paper or `live_readonly` run, validate it with `validate-replay-data`, and only then use it for sweeps or live-trading readiness decisions.

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
- the shadow analysis pages remain useful through the paper track
- the Execution page remains the real venue/account diagnostics surface
- the rejection telemetry is meaningful enough to explain inactive periods
- the dashboard IA and UX have already been rebuilt around Overview, Execution, Logs, and analysis pages

## What still blocks real money

- real order submission
- real order-state ingestion and reconciliation
- cancel-all and disarm behavior
- restart recovery with open live state
- redemption handling
- key rotation before any funded live deployment

## Exact next implementation scope

When the project returns to real-money implementation, the code pass should implement the minimum safe `live_trading` slice:

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

## UI boundary

The major dashboard IA cleanup is complete enough for the next research run. Future UI work should be limited to problems discovered during operation unless the real trading path introduces new safety requirements. Do not design destructive live controls beyond their gated presentation until the order path exists end to end.

## Operational notes to carry forward

- The readonly soak is healthy, but long runs grow SQLite materially because of feed capture.
- Future long runs should use replay-grade public capture and validate early, then manage storage by pruning/export discipline rather than downgrading capture quality.
- The Polymarket private key and relayer API key were exposed during development and must be rotated before any real-money use.

## Ready-to-start checklist for the next pass

- new replay-grade research run collected and validated as `sweep_grade`
- new run analyzed, including drawdown/halt behavior and strategy attribution
- readonly soak reviewed and accepted
- live wallet and relayer credentials rotated
- next pass scoped to the minimum live-trading path above
- full tests, docs, review, and gates required again before any funded deployment
