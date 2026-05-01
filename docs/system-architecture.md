# System Architecture

This document describes the durable shape of the workspace. It should describe existing system structure, not active implementation plans.

## Services

The workspace has five major components:

- `bots/paint`: the BTC Up/Down trading bot. It runs paper trading, live-readonly shadow trading, backtests, replay-data validation, parameter sweeps, settlement verification, and derived-data build tools.
- `agent`: a monitoring service that reads a bot SQLite database in read-only WAL mode and exposes REST plus WebSocket APIs.
- `dashboard/server`: the authenticated dashboard backend. It manages users, JWT sessions, static frontend serving, and proxying to one or more agents.
- `dashboard/client`: the React dashboard. It exposes Overview, Execution, Logs, Equity, Trades, Signals, and Strategies.
- `polymarket-sidecar`: the TypeScript authenticated Polymarket boundary. It owns proxy-wallet auth, readonly account/preflight checks, user-stream health, and future order/cancel/redeem routing.

The bot writes SQLite. The agent reads the bot DB. The dashboard server proxies authenticated frontend requests to agents. The dashboard client consumes REST data and WebSocket cache invalidations. The sidecar is local-only and private to the bot runtime.

## Execution Modes

The bot has three explicit execution modes:

- `paper`: shared strategy runtime with simulated execution.
- `live_readonly`: authenticated account and venue monitoring plus the shared shadow paper runtime. It does not place orders.
- `live_trading`: reserved for future real order flow. It is intentionally gated in the current tree.

The sidecar has real readonly-safe endpoints for `/health`, `/account`, and `/preflight`. The write endpoints `/orders`, `/cancel`, `/cancel-all`, and `/redeem-all` still return explicit not-implemented responses.

## Strategy Runtime

The paint bot evaluates three strategy families:

- `latency-arb`: buys the predicted side when Binance moves first and the Polymarket ask is still acceptable.
- `spread-capture`: buys both UP and DOWN when the two asks are cheap enough after fees. The legs are independent, so residual one-leg exposure is possible.
- `calm-persistence`: buys the currently winning side in quiet late-window regimes.

The portfolio router chooses one family per evaluation snapshot:

- `dislocation` maps to latency-arb.
- `structural_pair` maps to spread-capture.
- `calm` maps to calm-persistence.

Per-strategy sleeves and trend tracking prevent one family from consuming another family by shared bankroll or shared suppression state.

## Public Feeds

The bot consumes three public market feeds:

- Binance combined streams: `aggTrade`, `bookTicker`, and `depth@100ms`, with microsecond timestamps when available.
- Polymarket CLOB market WebSocket: order book snapshots, best-bid-ask updates, and price changes for UP and DOWN tokens.
- Polymarket RTDS Chainlink WebSocket: BTC/USD oracle context used for settlement and monitoring.

All feed connection attempts are bounded. Binance and CLOB have no-message watchdog reconnects. These transport safeguards reduce stale-data downtime, but they must not loosen stale-data gates or permit trading blind.

## Core Bot Modules

Important paint modules:

- `live.rs`: shared runtime for `paper` and `live_readonly`, with `live_trading` gated.
- `live_readonly.rs`: readonly preflight, live sessions, account snapshots, reconciliation, and operator rollups.
- `config.rs`: env and sweep configuration, execution mode, live budget caps, and reserve knobs.
- `strategy_cycle.rs`: shared evaluation, routing, trend suppression, spread affordability, signal persistence, and order submission into the execution engine.
- `executor.rs`: shared paper execution model with simulated arrival latency, partial fills, misses, and execution metrics.
- `signal_features.rs`: shared signal-feature engine used by live paper and backtests.
- `market_discovery.rs`: Gamma slug discovery, active window activation, metadata capture, and settlement fetches.
- `bankroll.rs`: per-strategy half-Kelly sizing, sleeves, caps, drawdown pause, and pending-settlement reserve accounting.
- `position_manager.rs`: trade lifecycle, duplicate guards, and authoritative settlement.
- `fees.rs`: dynamic taker fee modeling and historical fee resolution.
- `live_sidecar.rs`: typed Rust client for the local sidecar.

Backtesting lives under `bots/paint/src/backtest/`. Database ownership lives under `bots/paint/src/db/`.

## Dashboard IA

The dashboard is split into Monitor and Analysis:

- Overview: operator triage page with performance summary, current market, open trades, execution snapshot, and recent outcomes.
- Execution: process, mode, account readiness, reconciliation, live detail surfaces, and future controls.
- Logs: operator event stream with search and filters.
- Equity, Trades, Signals, Strategies: shadow-performance analysis pages.

`/execution` is canonical. `/trading`, `/live`, and `/stats` remain compatibility redirects.

## Frontend Structure

The dashboard client uses React, Vite, TanStack React Query, Zustand, and lightweight-charts. It is installable as a PWA and supports responsive desktop and mobile layouts.

Key frontend areas:

- `pages/`: route pages.
- `hooks/`: API and cache hooks.
- `lib/routes.ts`: canonical route metadata and redirects.
- `lib/trading-summary.ts`: presentation helpers for process, runtime, trading, health, capabilities, and alert labels.
- `components/layout/`: shell, header, navigation, and logo.
- `components/ui/dashboard-primitives.tsx`: shared dashboard UI primitives.

## Data Flow

The normal flow is:

1. The bot discovers a 5-minute market and subscribes to public feeds.
2. The shared strategy cycle evaluates features and routes one strategy family per snapshot.
3. Paper execution queues a simulated taker-style order with configured arrival latency.
4. The bot records signals, signal metrics, trades, outcomes, balance changes, feed health, and rejection summaries.
5. The agent reads SQLite and serves dashboard APIs.
6. The dashboard visualizes operator state and analysis.

In `live_readonly`, the bot also records live sessions, account snapshots, and reconciliation state from the sidecar while continuing the shadow paper track.
