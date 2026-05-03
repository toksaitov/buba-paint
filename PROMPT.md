# Auth and No-Order Sidecar Soak Handoff

You are helping validate the `buba-paint` live-readiness host path. The goal is to independently test Polymarket CLOB authentication, deploy the current local repo to the Finland host, and run a no-order `live_readonly` soak. Do not place orders, cancel orders, redeem, arm live trading, or run a funded canary.

## Grounding Sources

Read these first and treat official docs as authoritative:

- Polymarket authentication docs: https://docs.polymarket.com/api-reference/authentication
- Official Polymarket agent skill authentication guide: https://github.com/Polymarket/agent-skills/blob/main/authentication.md
- Repository instructions: `AGENTS.md`, then `CLAUDE.md`
- Current active plan/status: `LIVE_TRADING_PLAN.md`
- Host ops docs: `docs/deployment-and-ops.md`
- Commands/config docs: `docs/commands-and-config.md`
- Sidecar code: `polymarket-sidecar/src/config.ts`, `polymarket-sidecar/src/provider.ts`, `polymarket-sidecar/src/index.ts`

## Safety Rules

- Do not run `EXECUTION_MODE=live_trading`.
- Do not arm trading.
- Do not call dashboard mutation controls.
- Do not call sidecar order/cancel/redeem endpoints except to confirm they are not used.
- Do not write secrets into git-tracked files, logs, Markdown reports, DB rows, screenshots, or evidence bundles.
- Do not delete prior remote runtimes or primary DBs. Create a fresh release and runtime.
- If any `live_order_intents`, `live_orders`, `live_fills`, or `live_redemptions` row appears during this work, stop immediately and report it as a blocker.

## Access and Credential Handoff

SSH target:

```bash
ssh buba-paint-fin
```

Public account identifiers:

```text
POLYMARKET_PROXY_WALLET=0xE7C092ffa4c73EA874d8309cFC0e8915cb348616
POLYMARKET_FUNDER=0xE7C092ffa4c73EA874d8309cFC0e8915cb348616
POLYMARKET_SIGNATURE_TYPE=1
SIGNER_ADDRESS=0x1B5dC3BDA951494a2c00b84FCF4404baa69876D1
```

The secret-bearing env file is intentionally not embedded in this prompt. It should be handed to you separately or copied from:

```bash
.secrets/buba-paint-live-sidecar.env
```

Expected secret keys in that file:

```text
POLYMARKET_PRIVATE_KEY
POLYMARKET_API_KEY
POLYMARKET_API_SECRET
POLYMARKET_API_PASSPHRASE
POLYMARKET_PROXY_WALLET
POLYMARKET_FUNDER
POLYMARKET_SIGNATURE_TYPE
POLYMARKET_CLOB_HOST
```

Relayer API keys are not required for the no-order readonly soak. If you inspect redemption readiness, keep it fail-closed unless the builder relayer credentials required by the installed SDK are complete.

Before using the secrets, verify they are ignored:

```bash
git check-ignore -v .secrets/buba-paint-live-sidecar.env
chmod 600 .secrets/buba-paint-live-sidecar.env
```

## Local Preparation

Run from the repo root:

```bash
cd /Users/toksaitov/Desktop/buba-paint
git status --short
python3 -m py_compile scripts/live-readiness-host-soak.py
cd polymarket-sidecar && npm test && npm run build
cd ..
make live-readiness-local
```

If any command fails, stop and fix the blocker before touching the host.

## Independent Auth Probe Requirement

Before running the soak automation, write or run a minimal auth probe outside the sidecar code. Use the official docs and skill above. Test at least:

- L1 signer/proxy/funder config shape.
- CLOB L2 credential derivation or direct configured L2 credential use.
- `getServerTime`.
- `getBalanceAllowance`.
- `getOpenOrders` or equivalent current CLOB V2 open-order read.
- A short stability loop of at least 20 signed account/open-order reads.

Run this probe locally first, then on `buba-paint-fin` using the same env values. Keep outputs redacted. A single success is not enough; account and open-order reads must be stable enough for a soak.

## Remote Cleanup and Secret Install

Do not remove historical runtimes. Stop only active services and reset failed units:

```bash
ssh buba-paint-fin 'set -euo pipefail
systemctl --user stop buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
systemctl --user reset-failed buba-dashboard.service buba-agent.service buba-paint-bot.service buba-polymarket-sidecar.service 2>/dev/null || true
mkdir -p /root/buba-paint-live/config /root/buba-paint-live/logs /root/buba-paint-live/releases /root/buba-paint-live/runtime
systemctl --user list-units "buba-*" --all --no-pager || true
ps -eo pid=,etime=,args= | awk "/buba-paint live|buba-agent|buba-dashboard|polymarket-sidecar|node dist\\/index.js/ && !/awk/ {print}" || true
'
```

Install the sidecar env securely:

```bash
scp .secrets/buba-paint-live-sidecar.env buba-paint-fin:/tmp/buba-paint-live-sidecar.env
ssh buba-paint-fin 'set -euo pipefail
install -m 600 /tmp/buba-paint-live-sidecar.env /root/buba-paint-live/config/sidecar.env
rm -f /tmp/buba-paint-live-sidecar.env
'
```

Verify no repo-root DB/WAL/SHM garbage exists on the host:

```bash
ssh buba-paint-fin 'find /root/buba-paint-live -maxdepth 2 \( -name "*.db" -o -name "*.db-wal" -o -name "*.db-shm" \) -print'
```

Runtime DBs should live only under fresh `runtime/...` directories.

## Dry Run the Deployment Plan

Inspect the host-soak command plan without mutating the host:

```bash
LIVE_HOST_SOAK_ARGS="--host buba-paint-fin --duration-seconds 300 --poll-seconds 60 --output-dir data/experiments/replay-grade-readonly-soak-colleague-001 --release-stamp colleague-$(date -u +%Y%m%d-%H%M%SZ) --dry-run" \
  make live-readiness-host-soak
```

Confirm the plan stages a fresh release under:

```text
/root/buba-paint-live/releases/<stamp>
```

and a fresh runtime under:

```text
/root/buba-paint-live/runtime/soak-...
```

The deployment must exclude `.git`, `target`, `data`, `runs`, `node_modules`, local DBs, local logs, and generated garbage.

## Five-Minute No-Order Soak

Run the first short soak:

```bash
LIVE_HOST_SOAK_ARGS="--host buba-paint-fin --duration-seconds 300 --poll-seconds 60 --output-dir data/experiments/replay-grade-readonly-soak-colleague-001 --release-stamp colleague-$(date -u +%Y%m%d-%H%M%SZ)" \
  make live-readiness-host-soak
```

Required acceptance for the five-minute run:

- Sidecar `/health` is ready or has no money/order/auth/account blocker.
- Bot runs only in `EXECUTION_MODE=live_readonly`.
- Sidecar, bot, agent, and dashboard are supervised by user systemd.
- `live-preflight` passes.
- Dashboard and agent health endpoints pass.
- SQLite `PRAGMA quick_check` returns `ok`.
- `validate-replay-data` returns `replay_quality=sweep_grade` over the captured interval.
- `live_order_intents`, `live_orders`, `live_fills`, and `live_redemptions` are all zero.
- Evidence contains no secret values.
- Services are stopped at closeout unless explicitly using `--skip-stop`.

If the five-minute run fails any acceptance check, stop and write a concise blocker report. Do not run the 90-minute soak.

## Ninety-Minute No-Order Soak

Only after the five-minute soak passes, run:

```bash
LIVE_HOST_SOAK_ARGS="--host buba-paint-fin --duration-seconds 5400 --poll-seconds 300 --output-dir data/experiments/replay-grade-readonly-soak-colleague-002 --release-stamp colleague-$(date -u +%Y%m%d-%H%M%SZ)" \
  make live-readiness-host-soak
```

Acceptance is the same as the five-minute soak, plus:

- No sustained user-stream/account/venue degradation.
- Replay-grade required feed classes remain present across the interval.
- No unintended stale process from another release is running.
- Remote runtime is preserved.
- Non-secret evidence is copied locally under the selected `data/experiments/...` output directory.

## Manual Host Checks

Useful checks during or after the soak:

```bash
ssh buba-paint-fin 'systemctl --user status buba-polymarket-sidecar.service buba-paint-bot.service buba-agent.service buba-dashboard.service --no-pager'
ssh buba-paint-fin 'curl -sS http://127.0.0.1:3210/health | python3 -m json.tool'
ssh buba-paint-fin 'ps -eo pid=,etime=,args= | awk "/buba-paint live|buba-agent|buba-dashboard|polymarket-sidecar|node dist\\/index.js/ && !/awk/ {print}"'
ssh buba-paint-fin 'tail -n 120 /root/buba-paint-live/logs/sidecar.log 2>/dev/null || true'
```

For a known runtime DB:

```bash
ssh buba-paint-fin 'DB=/root/buba-paint-live/runtime/<runtime-name>/paint.db
sqlite3 "$DB" "PRAGMA quick_check;"
sqlite3 "$DB" "select source,event_type,count(*) from feed_events group by source,event_type order by source,event_type;"
sqlite3 "$DB" "select (select count(*) from live_order_intents),(select count(*) from live_orders),(select count(*) from live_fills),(select count(*) from live_redemptions);"
'
```

## Deliverables

Produce:

- A short auth probe report with redacted commands/results.
- Five-minute soak manifest and notes.
- If accepted, ninety-minute soak manifest and notes.
- Exact remote release path and runtime path.
- Confirmation that services were stopped or, if left running intentionally, exact reason and process state.
- Any code patches needed to make the readonly path correct, with tests and build results.

Do not call the work complete if account/order truth is stale, CLOB auth is ambiguous, replay-grade validation fails, or the sidecar hides degraded state.
