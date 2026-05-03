# Soak Deliverables (Colleague 001)

## Auth probe

- Independent probe script: [polymarket-sidecar/scripts/clob-auth-probe.mjs](../../../polymarket-sidecar/scripts/clob-auth-probe.mjs)
- Probe report: [probes/probe-report.md](probes/probe-report.md)
- Run notes and per-host run table: [probes/notes.md](probes/notes.md)
- Per-host probe artifacts (redacted JSON):
  - `probes/buba-paint-ireland-001.json` (25 iter, 25/25 balance + 25/25 orders, ~30 ms)
  - `probes/buba-paint-ireland-002.json` (60 iter, 60/60 balance + 60/60 orders, ~31 ms)
  - `probes/buba-paint-fin-001.json` (25 iter, 25/25 balance + 25/25 orders, ~102 ms)
  - `probes/buba-paint-fin-002.json` (60 iter, 60/60 balance + 60/60 orders, ~102 ms)
- Local macOS probe artifacts at `/tmp/buba-clob-probe/local-001..004.json` (not committed): one pre-unblock failure, then 25 + 30 + 30 stable.

Total successful authenticated CLOB v2 reads since the May-2 blocker resolved: 255 balance + 255 orders, both signature_type 1 and 2 passing, zero failures, across macOS, AWS Ireland, and Hetzner Finland.

## Code patches

`scripts/live-readiness-host-soak.py` had two latent assumptions that broke a fresh-host soak on `buba-paint-fin`. Both are now fixed:

1. **Hard `run-013` baseline requirement.** The script previously refused to run unless `runtime/run-013/{sidecar,paint,agent,dashboard}.env` all existed under the remote root. That path was a Phase 8 artifact on the Ireland host; on Finland it does not exist and PROMPT.md's expected install path is `config/sidecar.env`. The patch checks `config/sidecar.env` first, falls back to `runtime/run-013/sidecar.env` only if needed, and treats the three other baseline env files as empty dicts when absent. Strategy and reserve env vars resolve to code defaults from `bots/paint/src/config.rs` (`MAX_DRAWDOWN_PCT=0.5`, `SIM_ORDER_LATENCY_MS=250`, `TAKER_FEE_RATE` from venue metadata, etc.).
2. **Missing `ADMIN_USER` / `ADMIN_PASSWORD` in dashboard.env.** With no run-013 baseline the dashboard env had no admin creds, so the soak script's `dashboard-summary` step crashed on `KeyError: 'ADMIN_USER'`. The patch generates `ADMIN_USER=admin` and a fresh `ADMIN_PASSWORD=secrets.token_urlsafe(32)` per soak, persists the password to `<runtime>/dashboard-admin-password.txt` (mode 0600, host-side only) and surfaces the path in `env-report.json` under `dashboard_login_password_written_to`.

`make lint`, `make comment-audit`, `make docs-audit`, and `make live-readiness-local` all pass after the patch. The 5-minute soak post-patch passed every hard acceptance check; the 90-minute soak is the second-stage gate.

## 5-minute soak: passed acceptance

Run `colleague-20260503-054927Z` against `buba-paint-fin`, soak window `2026-05-03T05:53:30Z` to `2026-05-03T05:58:59Z`.

Verdict: `passed=true` in `manifest.json`. Every hard PROMPT.md gate satisfied:

- Sidecar `/health` ready: `live-preflight` returned `auth_status=ok`, `allowance_status=ok`, `clock_status=ok`, `available_cash_usd=99.166994`, `clob_contract_version=v2`, `collateral_token=pUSD`, `geoblock.blocked=false`, two BTC up/down 5m markets `acceptingOrders=true` from `metadataSource=clob_v2`.
- Bot in `EXECUTION_MODE=live_readonly`. `live_sessions` recorded `live_readonly:readonly_ready:1`.
- Sidecar / bot / agent / dashboard all supervised via user systemd; all health endpoints `200 ok` across pre-soak, five poll intervals, post-soak.
- SQLite `PRAGMA quick_check` returned `ok` at post-soak and post-stop.
- `validate-replay-data` returned `replay_quality=sweep_grade` over the 329 s soak window. All required feed classes present: `binance_agg_trade=610 rows`, `binance_book_ticker=6844 rows`, `binance_depth=1305 rows`, `chainlink_price=320 rows`, `clob_up_top_of_book=1082 rows`, `clob_down_top_of_book=1080 rows`. Total `feed_event_rows=13067`, legacy fallback `tick_rows=1316`.
- `live_order_intents=0`, `live_orders=0`, `live_fills=0`, `live_redemptions=0` at post-soak. Acceptance check (`17-remote-acceptance-check.log`) printed `acceptance_check=ok`.
- Services stopped at closeout. `post-stop-processes.txt` shows zero loaded `buba-*` user units. `post-stop-health.json` returns empty bodies (services down, expected).
- Builder relayer creds intentionally absent → `redemption_readiness=unavailable_missing_builder_credentials` (PROMPT.md says fail-closed for redemption is acceptable on a readonly soak).

Soft anomaly only: `dashboard-summary` step `rc=1` because dashboard.env had no `ADMIN_USER`. Patched in script for the 90-min run.

Artifacts in this directory:

- `manifest.json` (release/runtime paths, command list, verdict).
- `notes.md` (script-generated soak summary).
- `01-remote-bootstrap-dirs.log` through `23-post-stop-db.log` (per-step raw output).
- `live-preflight.json`, `replay-quality.txt`, `env-report.json`.
- `pre-soak-*.txt`, `post-soak-*.txt`, `post-stop-*.txt` (health, processes, log tails, DB row counts at each phase).
- `dashboard-trading-summary.json` (empty for the 5-min run; populated on the 90-min after the patch).

## 90-minute soak

Conditional on the 5-minute soak passing all acceptance checks. Same artifact set, written under [../replay-grade-readonly-soak-colleague-002/](../replay-grade-readonly-soak-colleague-002/).

## Remote release / runtime paths

Captured per run in the manifest. The 5-minute run targets:

- Release: `/root/buba-paint-live/releases/phase9-readonly-soak-colleague-<stamp>`
- Runtime: `/root/buba-paint-live/runtime/soak-001-colleague-<stamp>`
- Sidecar env (mode 0600): `/root/buba-paint-live/config/sidecar.env`

## Services state at closeout

The soak script stops `buba-dashboard.service`, `buba-agent.service`, `buba-paint-bot.service`, and `buba-polymarket-sidecar.service` at closeout (no `--skip-stop`). Post-stop processes/health captures verify zero `buba-*` user units active and zero stray binaries.

## Packages installed via sudo on hosts

- `buba-paint-fin` (Finland): no apt installs. The soak script reuses the existing host toolchain (`cargo`, `npm`, `node`).
- `buba-paint` (Ireland): no apt installs for this work. Probe used `npm install --silent --no-audit --no-fund` of `@ethersproject/wallet` under `/tmp/buba-clob-probe/` (no sudo).

## Safety guarantees observed

- `EXECUTION_MODE=live_readonly` set by the soak script, never `live_trading`.
- `live_order_intents`, `live_orders`, `live_fills`, `live_redemptions` checked at acceptance time (must all be zero).
- No live order / cancel / redeem / arm calls from any path during the run.
- Sidecar env file installed at mode 0600; deleted from `/tmp` after install.
- Probe env files copied to `/tmp/buba-clob-probe/sidecar.env` on Ireland and Finland for the probe runs, then deleted immediately after the probes finished. No env file persisted on either host.
- Probe artifacts redacted before being copied back into the repo.
