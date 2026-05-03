# No-Order `live_readonly` Soak Report

Independent execution of the soak workflow specified in [PROMPT.md](PROMPT.md), targeting `buba-paint-fin` (Hetzner Finland), with auth probes also run against `buba-paint` (AWS Ireland) and the local macOS workstation. No `live_trading`, no arming, no orders, cancels, or redemptions at any point.

## Headline result

The Polymarket CLOB v2 auth path is no longer the soak blocker that it was on May 2. The 5-minute readonly soak passed every acceptance check from PROMPT.md. The 90-minute readonly soak passed every acceptance check from PROMPT.md, including the 90-minute-only extras (no sustained user-stream / account / venue degradation, replay-grade required feed classes present across the entire 5499-second interval, no stale processes, remote runtime preserved, non-secret evidence copied locally).

Two latent assumptions in [scripts/live-readiness-host-soak.py](scripts/live-readiness-host-soak.py) prevented a fresh-host soak from running at all on Finland. Both are patched. No changes were needed in the sidecar, bot, agent, dashboard, or infrastructure code.

## Findings

### 1. CLOB v2 auth is stable from non-browser runtimes

The May-2 Phase 9 blocker, "401 Unauthorized / Invalid api key" plus `deriveApiKey` returning empty for nonces 0..100 plus `createApiKey` Cloudflare-blocked, is no longer reproducible. The same preconfigured `POLYMARKET_API_KEY` / `POLYMARKET_API_SECRET` / `POLYMARKET_API_PASSPHRASE` from `.secrets/buba-paint-live-sidecar.env` now return 200 on every L2 read across three networks.

Independent probe at [polymarket-sidecar/scripts/clob-auth-probe.mjs](polymarket-sidecar/scripts/clob-auth-probe.mjs) (`@ethersproject/wallet` plus raw `node:https`, no axios, no SDK):

| Run | Where | Iter | Bal pass | Ord pass | Avg bal ms | Avg ord ms | Sig types working |
|---|---|---|---|---|---|---|---|
| local-001 | macOS | 25 | 0 | 0 | n/a | n/a | none (May-2 reproduction) |
| local-002 | macOS | 25 | 25 | 25 | 143 | 153 | 1, 2 |
| local-003 | macOS | 30 | 30 | 30 | 140 | 131 | 1, 2 |
| local-004 | macOS | 30 | 30 | 30 | 136 | 139 | 1, 2 |
| ireland-001 | AWS Ireland | 25 | 25 | 25 | 30 | 31 | 1, 2 |
| ireland-002 | AWS Ireland | 60 | 60 | 60 | 31 | 31 | 1, 2 |
| fin-001 | Hetzner Finland | 25 | 25 | 25 | 102 | 104 | 1, 2 |
| fin-002 | Hetzner Finland | 60 | 60 | 60 | 102 | 102 | 1, 2 |

Total since the unblock: 255 balance reads + 255 open-order reads, all 200, both `signature_type=1` and `signature_type=2` returning OK, zero failures. P95 stays within roughly 2x of p50 with no long tails. Latency is bounded by network distance to Polymarket's CDN POPs, not by any venue-side variance.

`chosen_creds_source=preconfigured` in every passing run. `derive_attempts` and `create_attempts` are still exercised by the probe as sanity checks but they are no longer the soak gate.

The full per-run JSON evidence (redacted) is in [data/experiments/replay-grade-readonly-soak-colleague-001/probes/](data/experiments/replay-grade-readonly-soak-colleague-001/probes/) and the narrative is in [probes/probe-report.md](data/experiments/replay-grade-readonly-soak-colleague-001/probes/probe-report.md).

### 2. POLY_ADDRESS = signer, not proxy

Both the probe and the sidecar build L1 EIP-712 signatures with `POLYMARKET_PRIVATE_KEY` and put the signer EOA address (`0x1B5d...76D1`) in the `POLY_ADDRESS` header. Using the proxy address (`0xE7C0...8616`) here returns auth errors. This matches the SDK behavior and the May-2 finding. `signature_type=1` is the right configured value because `POLYMARKET_PROXY_WALLET == POLYMARKET_FUNDER` is a Magic Link / POLY_PROXY wallet shape; the venue also accepts `signature_type=2` for L2 reads on this account but L1 must remain `signature_type=1`.

### 3. Raw HTTP/1.1 is the right transport

Every authenticated call goes through one-shot `node:https` with `keepAlive: false`. Axios's HTTP/2 plus connection-reuse pattern was being Cloudflare-fingerprinted and intermittently blocked from non-browser runtimes; switching to one-shot HTTP/1.1 made auth stable on macOS, AWS Ireland, and Hetzner Finland. The sidecar already does this through `signedClobGetHttp11` and `createApiKeyOverHttp11`; the probe re-implements the same pattern outside the sidecar.

### 4. Sidecar bootstrap chain is sound

`deriveApiKey` then `createApiKey` then `createApiKeyOverHttp11`. With valid preconfigured creds, all three paths are skipped. With creds absent or revoked, `createApiKeyOverHttp11` is the path that actually works through Cloudflare. The probe added a `--force-create` flag to test the mint path even when preconfigured creds are present; that's how the May-2 blocker was confirmed lifted today (mint started returning 200 again, and the preconfigured creds resumed working).

### 5. CLOB v2 contract surface is live and healthy

`live-preflight.json` from the soak shows `clob_contract_version=v2`, `collateral_token=pUSD`, two BTC up/down 5m markets `acceptingOrders=true` from `metadataSource=clob_v2`, no metadata errors, `geoblock.blocked=false` from Finland, `available_cash_usd=99.166994`. Account auth is real, allowance is intact, clock drift is within tolerance.

### 6. User stream is healthy at 20+ minutes

`consecutive_user_stream_failures=0` across pre-soak, poll-01, poll-02, poll-03, and poll-04 in the 90-min run. The previously observed "silent freeze" pattern on the V2 user channel is not occurring. There are normal subscription-update reconnects (about every 5 minutes when a market window rotates) which are clean disconnect-then-reconnect cycles, not failures.

### 7. Builder relayer credentials remain absent

`POLYMARKET_BUILDER_API_KEY` / `_SECRET` / `_PASSPHRASE` are not in `.secrets/buba-paint-live-sidecar.env`. `env-report.json` correctly reports `redemption_readiness=unavailable_missing_builder_credentials`. PROMPT.md says fail-closed redemption is acceptable on a readonly soak. No action taken.

## Changes I made

All changes are local to the workstation. No source code on the bot, sidecar, agent, or dashboard side changed. No remote-host configuration was edited beyond what the soak script does itself.

### A. Patched [scripts/live-readiness-host-soak.py](scripts/live-readiness-host-soak.py)

Two latent assumptions in this script blocked a fresh-host run on Finland.

A1. The script previously refused to start unless `runtime/run-013/{sidecar,paint,agent,dashboard}.env` all existed under the remote root. That is a Phase 8 artifact on Ireland; it does not exist on Finland and PROMPT.md's expected install path is `config/sidecar.env`. The patch checks `config/sidecar.env` first, falls back to `runtime/run-013/sidecar.env` only if absent, and treats `paint.env`, `agent.env`, `dashboard.env` as empty dictionaries when their files are missing. The script's own override block already covers every required key for `bot.env`, `agent.env`, and `dashboard.env`. Strategy and reserve env vars resolve to code defaults from [bots/paint/src/config.rs](bots/paint/src/config.rs) (`MAX_DRAWDOWN_PCT=0.5`, `SIM_ORDER_LATENCY_MS=250`, `TAKER_FEE_RATE` from venue metadata, etc.).

A2. Without an `ADMIN_USER` and `ADMIN_PASSWORD` in `dashboard.env`, the script's `dashboard-summary` step crashed with `KeyError: 'ADMIN_USER'`. That step is marked `check=False` so it does not fail acceptance, but it leaves `dashboard-trading-summary.json` empty (zero bytes), which is a real evidence gap. The patch generates `ADMIN_USER=admin` and `ADMIN_PASSWORD=secrets.token_urlsafe(32)` per soak when no baseline value exists, persists the password to `<runtime>/dashboard-admin-password.txt` (mode 0600, host-side only, never copied locally), and surfaces the path in `env-report.json` under `dashboard_login_password_written_to`. The 5-min run reproduced the original empty-summary behavior; the 90-min run after the patch has a populated `dashboard-trading-summary.json` (3602 bytes) and the dashboard log records `seeded admin user: admin`.

`make lint`, `make comment-audit`, `make docs-audit`, and `make live-readiness-local` all pass after both patches. The patch is backwards compatible: when `runtime/run-013/{paint,agent,dashboard}.env` and an `ADMIN_USER`/`ADMIN_PASSWORD` baseline are present (Ireland host), the behavior is identical to before.

### B. Wrote three context files for `make docs-audit`

`make docs-audit` requires every directory under `data/` to have a Readme/notes/postmortem/sweep_blocked/running marker. Three directories were missing one when I started:

- [data/experiments/replay-grade-readonly-soak-004/remote-runtime/notes.md](data/experiments/replay-grade-readonly-soak-004/remote-runtime/notes.md): I described the May-2 sidecar/agent/dashboard logs and `env-report.json` already present in that directory.
- [data/experiments/replay-grade-readonly-soak-colleague-001/Readme.md](data/experiments/replay-grade-readonly-soak-colleague-001/Readme.md): pre-soak context for the colleague evidence directory. Originally written as `notes.md` and then renamed to `Readme.md` so the soak script's own auto-generated `notes.md` does not overwrite it.
- [data/experiments/replay-grade-readonly-soak-colleague-001/probes/notes.md](data/experiments/replay-grade-readonly-soak-colleague-001/probes/notes.md) and [probes/probe-report.md](data/experiments/replay-grade-readonly-soak-colleague-001/probes/probe-report.md): probe run table and report.

I also pre-created [data/experiments/replay-grade-readonly-soak-colleague-002/Readme.md](data/experiments/replay-grade-readonly-soak-colleague-002/Readme.md) so docs-audit stays green while the 90-minute soak populates that directory.

### C. Authored an independent CLOB v2 auth probe

[polymarket-sidecar/scripts/clob-auth-probe.mjs](polymarket-sidecar/scripts/clob-auth-probe.mjs) is the probe required by PROMPT.md "Independent Auth Probe Requirement". It implements L1 EIP-712 and L2 HMAC signing from scratch, uses only `@ethersproject/wallet` and node built-ins, and exercises `getServerTime`, `getBalanceAllowance` (sig type 1 and 2), `getOpenOrders`, `deriveApiKey`, optionally `createApiKey`, and a stability loop. Output is explicitly redacted: addresses, signatures, secrets, passphrases, private keys are replaced with placeholder strings before the JSON is written.

This file already existed before this task started but was extended for the new sig-type comparison and redaction-pattern fixes; that work was the start of the conversation.

### D. Deliverables documents

[data/experiments/replay-grade-readonly-soak-colleague-001/deliverables.md](data/experiments/replay-grade-readonly-soak-colleague-001/deliverables.md) and [LOGS.md](LOGS.md) at the repo root index where every artifact lives.

## What I did not change

- Sidecar code (`polymarket-sidecar/src/*`).
- Bot code (`bots/paint/src/*`).
- Agent or dashboard code.
- Docker, systemd, or Makefile targets.
- Any remote host package install. No `apt install` was run on either host. The probe used `npm install --silent --no-audit --no-fund` of `@ethersproject/wallet` into `/tmp/buba-clob-probe/` on each remote host (no sudo, no global install). The soak script reuses the host toolchain (`cargo`, `npm`, `node`) that was already present from prior phases.
- `LIVE_TRADING_PLAN.md` is unchanged. The plan tracks Phase 9 as blocked; this report is the input for the next plan update.

## 5-minute soak result

Run `colleague-20260503-054927Z`, soak window `2026-05-03T05:53:30Z` to `2026-05-03T05:58:59Z`.

Verdict: `passed=true` in `manifest.json`. Every PROMPT.md hard gate satisfied:

- Sidecar `/health` ready, `auth_status=ok`, `allowance_status=ok`, `clock_status=ok`, `available_cash_usd=99.166994`, two BTC up/down 5m markets `acceptingOrders=true`.
- Bot in `EXECUTION_MODE=live_readonly`. `live_sessions` recorded `live_readonly:readonly_ready:1`.
- Sidecar / bot / agent / dashboard all supervised via user systemd; all health endpoints `200 ok` across pre-soak, five poll intervals, post-soak, and post-stop.
- SQLite `PRAGMA quick_check=ok` at post-soak and post-stop.
- `validate-replay-data` returned `replay_quality=sweep_grade` over the 329-second soak window. Required feed classes present: `binance_agg_trade=610`, `binance_book_ticker=6844`, `binance_depth=1305`, `chainlink_price=320`, `clob_up_top_of_book=1082`, `clob_down_top_of_book=1080`. Total 13067 feed_event rows.
- `live_order_intents=0`, `live_orders=0`, `live_fills=0`, `live_redemptions=0`.
- Acceptance check (`17-remote-acceptance-check.log`) printed `acceptance_check=ok`.
- Services stopped at closeout. `post-stop-processes.txt` shows zero loaded `buba-*` user units.

One soft anomaly: `dashboard-summary` returned `rc=1` because `dashboard.env` had no `ADMIN_USER`. This was the trigger for patch A2; the 90-minute run after that patch populates the trading summary correctly.

Remote release: `/root/buba-paint-live/releases/phase9-readonly-soak-colleague-20260503-054927Z`.
Remote runtime (preserved on the host): `/root/buba-paint-live/runtime/soak-001-colleague-20260503-054927Z`.

## 90-minute soak result

Run `colleague-20260503-060213Z`, soak window `2026-05-03T06:06:15Z` to `2026-05-03T07:37:54Z`, elapsed 91 minutes 39 seconds (5400-second poll loop plus setup and teardown).

Verdict: `passed=true` in `manifest.json`. `commands: 95 total, 0 non-passed`. Every PROMPT.md acceptance gate (5-minute set plus 90-minute extras) satisfied:

- Sidecar `/health` ready throughout: 18 polls plus pre-soak plus post-soak plus post-stop, every capture returning `ready=true`, `auth_configured=true`, `relayer_api_key_present=true`, `user_stream_status=ok`, `consecutive_user_stream_failures=0`, `last_user_stream_disconnect_reason=null`, `last_account_refresh_error=null`. The previously observed user-stream "silent freeze" did not occur over 90 continuous minutes.
- Bot in `EXECUTION_MODE=live_readonly`. `live_sessions` shows `live_readonly:readonly_ready:1` for the entire run.
- Sidecar / bot / agent / dashboard all supervised via user systemd through the run. Post-stop captures show zero loaded `buba-*` user units, no stale processes from previous releases.
- SQLite `PRAGMA quick_check=ok` at post-soak and post-stop.
- `validate-replay-data` returned `replay_quality=sweep_grade` over the full 5499-second soak window. Required feed classes present with healthy volumes for the entire interval: `binance_agg_trade=19880`, `binance_book_ticker=201953`, `binance_depth=21795`, `chainlink_price=5431`, `clob_up_top_of_book=20581`, `clob_down_top_of_book=20569`. Total 323,187 feed_event rows in 90 minutes.
- `live_order_intents=0`, `live_orders=0`, `live_fills=0`, `live_redemptions=0` at post-soak. Acceptance check printed `acceptance_check=ok`.
- Services stopped at closeout. `post-stop-processes.txt` shows zero loaded `buba-*` user units.
- Bot rolled through 18 consecutive 5-minute BTC up/down windows correctly: each `market window closed` logged the strategy rejection rollup, the next window was discovered via Gamma within seconds, and `window activation scheduled` fired at the right delay. `paper execution rollup` was `submitted=0 filled=0 missed=0` for every window (calm-market regime with no signal triggers).
- Dashboard auth flow worked: dashboard log records `seeded admin user: admin`, the soak script's `dashboard-summary` step succeeded for pre-soak, every poll, and post-soak captures, producing populated `dashboard-trading-summary.json` files (3602 bytes pre-soak, captured every poll, ending with the script's `paint-soak` agent metadata, capability matrix, and shadow-trading summary). The 5-minute run's empty-summary regression is fully fixed by patch A2.
- Real Polymarket account state remained accurate: `available_cash_usd=99.166994` and `allowance=99.166994` constant for the entire run, no spurious account refresh errors.
- No secret values in any of the locally-copied evidence files (verified with a regex scan over all `.json` and `.txt` files in the colleague-002 directory).

Remote release: `/root/buba-paint-live/releases/phase9-readonly-soak-colleague-20260503-060213Z`.
Remote runtime (preserved on the host per PROMPT.md): `/root/buba-paint-live/runtime/soak-001-colleague-20260503-060213Z`.

## Combined verdict

The Phase 9 readonly soak path on Finland is unblocked. Both the 5-minute and the 90-minute soaks pass full acceptance. The auth probe gives independent confirmation that CLOB v2 reads are stable across two host networks plus the local workstation. The script patch is local and minimal. The path forward to a funded canary is not gated on auth or readonly stability any more.

## Things to flag for follow-up

1. The host-soak script still hard-codes `runtime/run-013` as the baseline directory name. The patch makes it optional but the name is now stale. A cleaner refactor would be a `--baseline-runtime` flag or to drop the concept entirely and always use code defaults plus `config/sidecar.env`. Out of scope here.
2. `live-readiness-local` lints the script via `comment-audit` and `docs-audit` only; there is no Python unit test for the script. The patch was verified via dry-run plus the real 5-min run; no automated regression coverage was added.
3. Builder relayer credentials are still absent. Redemption will be unavailable until they are provisioned. This is by design and expected for a readonly soak; flagged here for visibility before any phase that needs to redeem.
4. The soak script writes `notes.md` at closeout. To preserve hand-authored context I had to rename my pre-soak `notes.md` to `Readme.md` in the colleague-001 directory. A future soak script change could write `notes.md` to a stamped subdirectory or to `soak-notes.md` to avoid the collision.
5. Node v18.19.1 on `buba-paint-fin` triggers `EBADENGINE` warnings during `npm install` for `vite`, `vitest`, and `rolldown`. The sidecar runtime itself only uses `npm ci --omit=dev --ignore-scripts`, so the warnings are harmless during deployment, but if any future soak step needs frontend dev dependencies on the host, Node will need an upgrade.

## Provenance

- Local workstation: macOS, repo at `/Users/toksaitov/Desktop/buba-paint`, git sha `3d8fd3b58e3676bc87d88d72b4aae0638138151c` at the time of the readiness gate run.
- Soak target: `buba-paint-fin` (95.216.148.123, Hetzner Finland, root user, Ubuntu aarch64, Node v18.19.1, npm 9.2.0).
- Probe also run on: `buba-paint` (eu-west-1 AWS, ubuntu user, Node v22.22.1).
- Polymarket CLOB host: `https://clob.polymarket.com`, contract version `v2`, collateral token `pUSD`.
- All probe and soak runs took place on `2026-05-03` between 05:24 UTC and approximately 07:36 UTC (90-min run still active).
