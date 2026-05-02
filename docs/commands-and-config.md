# Commands and Configuration

This document keeps durable command and configuration guidance. Use [Readme.md](../Readme.md) for the shortest entrypoint.

## Build and Local Checks

```bash
cargo build
cargo build --release
cargo test
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
make lint
make comment-audit
make docs-audit
make test-fast
make test-integration
make test-slow
make test-e2e
make test-all
make coverage
make coverage-gate

cd dashboard/client && npm test
cd dashboard/client && npm run test:e2e
cd dashboard/client && npm run test:coverage
cd polymarket-sidecar && npm test
cd polymarket-sidecar && npm run build
```

## Local Stack

```bash
docker compose up -d
cd dashboard/client && npm run dev
```

Docker Compose starts a local paper stack with paint, agent, and dashboard. It does not start the Polymarket sidecar or authenticated `live_readonly` monitoring.

## Bot Commands

```bash
cargo run -p buba-paint --release -- init-db --db-path /tmp/paint.db --balance 200
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 200
cargo run -p buba-paint --release -- live --db-path /tmp/paint.db --balance 200 --set LATENCY_ARB_MAX_ASK=0.55
cargo run -p buba-paint --release -- live-preflight
cargo run -p buba-paint --release -- latency-probe --timeout-ms 3000
cargo run -p buba-paint --release -- db-footprint --db-path /tmp/paint.db
```

Use `/tmp` for local scratch DBs. Do not create DBs in the repository root.

## Backtest and Sweep

```bash
cargo run -p buba-paint --release -- backtest \
  --data data/market-data.db \
  --start 2026-02-20T03:13 \
  --end 2026-02-28T00:00 \
  --balance 200

cargo run -p buba-paint --release -- validate-replay-data \
  --data data/market-data.db \
  --start 2026-02-20T03:13 \
  --end 2026-02-28T00:00

cargo run -p buba-paint --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-20 \
  --end 2026-03-04 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0008,0.0010,0.0012 \
  --output data/sweeps/example/sweep.csv
```

`--sweep PARAM=start:end:step` generates a range. `--sweep PARAM=a,b,c` enumerates values. `--set PARAM=value` fixes a parameter without sweeping. Boolean values accept `true/false`, `1/0`, `yes/no`, and `on/off`; operator docs should prefer `true/false`.

Sweeps refuse inputs that are not `sweep_grade`. Backtests still run on descriptive archives, but they warn when the interval lacks replay-grade decision inputs.

## Settlement and Historical Data

```bash
cargo run -p buba-paint --release -- verify-settlements --db data/market-data.db --concurrency 15
cargo run -p buba-paint --release -- build-data --runs-dir runs --output data/market-data.db
cargo run -p buba-paint --release -- upgrade-history --runs-dir runs --from-run 4 --to-run 9 --rebuild-derived --output data/market-data.db
```

`verify-settlements` fetches actual Polymarket outcomes and compares them against locally derived settlements. `build-data` merges run DBs into derived data. `upgrade-history` performs additive historical upgrades and caches HTTP payloads under `data/backfill-cache/`.

## Exact-Run Replay

For exact pulled-run calibration, prefer observed resolution timing:

```bash
BACKTEST_SETTLEMENT_MODE=observed_market_resolution \
cargo run -p buba-paint --release -- backtest \
  --data /tmp/run-replay-data.db \
  --start 2026-04-04T20:15 \
  --end 2026-04-08T17:25 \
  --balance 200 \
  --set LATENCY_ARB_ENABLED=true \
  --set SPREAD_CAPTURE_ENABLED=true \
  --set CALM_PERSISTENCE_ENABLED=true
```

The pending-settlement reserve defaults are conservative. Override them only when intentionally comparing compatibility or risky modes. See [pending-settlement-modes.md](./pending-settlement-modes.md).

## Core Environment Knobs

Use [.env.example](../.env.example) as the canonical template.

Important groups:

- Execution mode: `EXECUTION_MODE=paper|live_readonly|live_trading`.
- Storage profile: `FEED_EVENT_STORAGE_PROFILE=replay_grade|compact|full_debug`.
- Feed freshness: `MAX_SIGNAL_FEED_AGE_MS`, `MAX_QUOTE_AGE_MS`, `WEBSOCKET_CONNECT_TIMEOUT_MS`, `BINANCE_NO_MESSAGE_RECONNECT_MS`, `CLOB_NO_MESSAGE_RECONNECT_MS`.
- Pending settlement: `PENDING_SETTLEMENT_FAMILY_RESERVE_FRACTION`, `PENDING_SETTLEMENT_GLOBAL_RESERVE_FRACTION`, `PENDING_SETTLEMENT_COUNTS_AS_OPEN_POSITION`, `BACKTEST_SETTLEMENT_MODE`.
- Live caps: `LIVE_SESSION_CASH_CAP_USD`, `LIVE_MAX_SINGLE_ORDER_USD`, `LIVE_MAX_OPEN_NOTIONAL_USD`, `LIVE_MAX_DAILY_LOSS_USD`, `LIVE_MAX_SESSION_DRAWDOWN_USD`, `LIVE_MIN_REQUIRED_CASH_USD`.
- Strategy toggles: `LATENCY_ARB_ENABLED`, `SPREAD_CAPTURE_ENABLED`, `CALM_PERSISTENCE_ENABLED`.
- Sidecar: `LIVE_SIDECAR_URL`, `POLYMARKET_PRIVATE_KEY`, `POLYMARKET_PROXY_WALLET`, `POLYMARKET_FUNDER`, `POLYMARKET_RELAYER_HOST`, `POLYMARKET_RELAYER_API_KEY`, `POLYMARKET_RELAYER_API_KEY_ADDRESS`, `POLYMARKET_BUILDER_API_KEY`, `POLYMARKET_BUILDER_SECRET`, `POLYMARKET_BUILDER_PASSPHRASE`.

The sidecar CLOB boundary uses `@polymarket/clob-client-v2`, pUSD collateral diagnostics, and proxy-wallet signature type `1` for the first account model. `POLYMARKET_FUNDER` defaults to `POLYMARKET_PROXY_WALLET` when omitted. Gasless redemption uses the builder relayer SDK path and stays fail-closed unless the configured credentials are complete.

`live_trading` starts disarmed and is local-verification only until deployment and final canary phases are complete. Do not treat the presence of live caps, sidecar credentials, callable sidecar write endpoints, or queued `live-control` commands as permission to deploy real money.

## Live Control CLI

The local CLI queues the same audited bot-applied commands as the dashboard Execution controls:

```bash
cargo run -p buba-paint --release -- live-control \
  --db-path /tmp/paint.db \
  arm \
  --actor operator \
  --reason "preflight gates passed"
```

Supported actions are `preflight`, `arm`, `disarm`, `stop-after-flat`, `kill-switch`, `cancel-all`, and `redeem-all`. Commands are written into the bot DB and applied by a running `EXECUTION_MODE=live_trading` process. The command is rejected if there is no active live-trading session.

The dashboard route `POST /api/bots/:id/live/control` proxies to the agent route `POST /api/live/control`. Only dashboard admins may submit it. The server injects the authenticated actor, and the bot remains the only process that applies controls or touches the sidecar.

## Live Closeout CLI

Terminal live sessions require an evidence package before a new funded run DB is started:

```bash
cargo run -p buba-paint --release -- live-closeout \
  --db-path /path/to/live-run/paint.db \
  --output-dir /path/to/closeout \
  --actor operator \
  --reason "session drawdown halt"
```

`live-closeout` writes `summary.json`, `manifest.json`, `db_integrity.txt`, `replay_quality.txt`, live ledger exports, control audit, and a `postmortem.md` stub. It records `live_closeout_exported` in the DB audit ledger. It does not make a halted DB re-armable; the next funded attempt must use a new run DB.

The closeout summary and manifest include observed replay-quality class, validation interval, and missing required public feed classes. If the interval is not `sweep_grade`, the postmortem stub labels the run descriptive-only.

Live-money risk defaults are:

- `LIVE_MAX_DAILY_LOSS_USD=15`
- `LIVE_MAX_SESSION_DRAWDOWN_USD=20`
- existing `MAX_DRAWDOWN_PCT`

An armed live session treats unresolved unknown order state, critical reconciliation, auth/geoblock/storage failure, or persistent account/user-stream/venue degradation as terminal blockers. `cancel-all` and `redeem-all` may still be queued for cleanup when their capability data says they are safe.

## Strategy Defaults

The current candidate settings live in [.env.example](../.env.example). Do not promote historical run settings without fresh replay-grade evidence.

For the first funded canary plan, code should support all strategy families, but runtime config should enable latency only:

```bash
LATENCY_ARB_ENABLED=true
SPREAD_CAPTURE_ENABLED=false
CALM_PERSISTENCE_ENABLED=false
```

That policy is active implementation planning and is tracked in [LIVE_TRADING_PLAN.md](../LIVE_TRADING_PLAN.md), not as current production behavior.
