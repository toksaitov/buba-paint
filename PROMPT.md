# Dashboard Monitor Expansion (Multi-Run Plan)

This is a multi-run UI/UX work order for the Buba Paint dashboard. The original brief bundled four genuinely separate efforts into one prompt; this rewrite splits them into four bounded, self-contained plan-mode runs that can be executed in sequence.

Recommended order:

1. Run 1: Logs Filter UX cleanup (frontend only). Smallest scope, clears in-flight uncommitted work, pure visual/responsive fix.
2. Run 2: Runtime Config page (full stack). Highest operator-pain payoff: "what knobs are running?" without SSH.
3. Run 3: Machine/System Health page (full stack). Second operator-pain payoff: "is the server healthy?" without SSH; security-sensitive due to `/proc` access.
4. Run 4: Favicon and App Icon pipeline (frontend assets). Most isolated; finicky platform quirks. Requires deliberate research, not raster guessing.

Each run lists its own scope, acceptance criteria, file list, and gates. Read the **Project Context** below before any run. Re-read the run-specific section before entering plan mode for that run. Each run is intended to ship as one PR with passing tests, no leftover dead code, and no destabilization of the bot or sidecar containers.

---

## Project Context (read before every run)

### Current state

The project is a Rust-first Polymarket BTC 5-minute trading workspace.

- `bots/paint`: bot runtime, SQLite DB, paper + live_readonly + disarmed live_trading.
- `agent`: read-only DB monitor and REST/WebSocket API.
- `dashboard/server`: authenticated dashboard backend and agent proxy.
- `dashboard/client`: React/Vite/Tailwind v4 dashboard.
- `polymarket-sidecar`: TypeScript authenticated Polymarket boundary (clob-client-v2 + builder-relayer + builder-signing + viem).

Deployed at `https://buba.toksaitov.com`. Host: `ssh buba-paint`. Remote layout: `~/buba-paint-live/current` and `~/buba-paint-live/runtime/<runtime-name>`. Runtime mode: `live_readonly`, never armed. Strategy mode: latency-arb only with Run 012 parameters, $100 shadow balance, calm/spread disabled.

The active live-money implementation plan is at the repo root in [LIVE_TRADING_PLAN.md](./LIVE_TRADING_PLAN.md). The canonical repository instruction file is [CLAUDE.md](./CLAUDE.md). Durable system docs start at [docs/Readme.md](./docs/Readme.md).

This work is observability-and-UX, not strategy or live-ordering. Do not change trading behavior. Read-only data surfaces may add small additive bot-side persistence (sanitized config snapshot, etc.).

### Safe iteration deploys (CRITICAL, applies to every run)

The owner wants to iterate on dashboard results without disturbing the running bot. Follow this discipline on the server.

- Do not run `scripts/deploy-docker.py` unless the owner explicitly requests a full stack redeploy.
- Do not run `docker compose down`.
- Do not run `docker compose up -d` without service names.
- Do not restart, recreate, or rebuild `paint` or `sidecar`.
- Do not edit/delete files under the active runtime directory except normal dashboard/agent/Caddy logs written by the services.

Compose context on the server:

- Release dir: `~/buba-paint-live/current`
- Compose files: `docker-compose.yml`, `docker-compose.live-readonly.yml`, `docker-compose.prod.yml`
- Project name: `buba-paint`
- Services: `paint`, `sidecar`, `agent`, `dashboard`, `caddy`
- Runtime bind mount: from `.env` as `BUBA_RUNTIME_DIR=~/buba-paint-live/runtime/<runtime-name>`

Standard helper prefix:

```bash
cd ~/buba-paint-live/current
COMPOSE='sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml'
```

Before partial redeploy, record the bot container start time; verify it is unchanged after.

```bash
ssh buba-paint 'sudo docker inspect -f "{{.State.StartedAt}}" buba-paint-paint-1'
```

For dashboard client/server changes:

```bash
ssh buba-paint '
set -euo pipefail
cd ~/buba-paint-live/current
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml build dashboard
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml up -d --no-deps dashboard
'
```

For agent-only API/read-model changes:

```bash
ssh buba-paint '
set -euo pipefail
cd ~/buba-paint-live/current
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml build agent
sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml up -d --no-deps agent
'
```

If a run touches both: rebuild `agent` first, then rebuild `dashboard`, both with `--no-deps`.

For Caddy config-only changes use `caddy reload` first; recreate the container only if reload fails.

For staging changed source from local to host without a full deploy, use `rsync` with narrow paths and `--exclude .git --exclude target --exclude data --exclude runs --exclude node_modules`. Stage into the existing release dir; do not create a new runtime dir for UI iteration.

Verification after every partial redeploy:

```bash
ssh buba-paint 'cd ~/buba-paint-live/current && sudo docker compose --env-file .env -f docker-compose.yml -f docker-compose.live-readonly.yml -f docker-compose.prod.yml ps'
ssh buba-paint 'sudo docker inspect -f "{{.State.StartedAt}}" buba-paint-paint-1'
curl -I http://buba.toksaitov.com
curl -fsS https://buba.toksaitov.com/health
ssh buba-paint 'tail -n 80 ~/buba-paint-live/runtime/*/dashboard.log'
ssh buba-paint 'tail -n 80 ~/buba-paint-live/runtime/*/agent.log'
```

Do not run SQLite `quick_check`, replay validators, or large DB scans as part of routine dashboard iteration. Those are closeout/offline diagnostics.

### Existing dashboard shape

Routes are defined in [dashboard/client/src/lib/routes.ts](dashboard/client/src/lib/routes.ts).

Monitor section: Overview, Execution, Logs.
Analysis section: Trend, Trades, Signals, Strategies.

Existing dashboard API pattern:

- Client calls `/api/bots/:id/...`.
- Dashboard server proxies to agent `/api/...`.
- Agent reads SQLite or process/log state and returns JSON.

Add new read-only endpoints with the same pattern.

### Cross-cutting implementation rules (apply to every run)

- Use small, additive APIs.
- No runtime table scans, no `quick_check` in health paths, no replay validators in the bot runtime.
- No secret exposure: never serialize private keys, relayer secrets, JWT/agent secrets, or sidecar credentials into APIs, snapshots, logs, or test fixtures.
- No dashboard mutation controls for the new pages. Read-only only.
- No Docker socket unless a deliberate, documented security decision says otherwise.
- Tests required for every new endpoint, hook, and page.
- Update durable docs after a run lands behavior, schema, or workflow changes. Do not append blindly.
- Every run must respect the Safe Iteration Deploys discipline above when shipping.

### Standard validation commands

Run the relevant subset for the run, plus the global gates:

```bash
cd dashboard/client
./node_modules/.bin/tsc --noEmit -p tsconfig.app.json
npm test -- --run
npm run lint
npm run build
```

Backend gates when a run touches Rust:

```bash
cargo build
cargo test -p buba-agent
cargo test -p buba-dashboard
cargo test -p buba-paint   # only when bot-side persistence changes
make lint
make comment-audit
make docs-audit
git diff --check
```

---

## Run 1: Logs Filter UX cleanup (frontend only)

### Why

The Logs page filter bar has been worked on and is still not solved. There is currently uncommitted in-flight work in [dashboard/client/src/pages/logs.tsx](dashboard/client/src/pages/logs.tsx) that attempts a two-row layout. The remaining failures, observed in the current diff:

- iPad portrait and narrow desktop widths still let the toggle group, line-count select, and the severity/source/event-type select row visually compete for space and feel ungrouped.
- The phone-only `Filters` / `Filters (N active)` button is visually heavy, takes a full-width row, and looks out of place against the dense console aesthetic of the rest of the dashboard.
- Earlier attempts moved the mess to different breakpoints rather than resolving the actual layout intent.

### Scope (in)

- One file is the primary edit target: [dashboard/client/src/pages/logs.tsx](dashboard/client/src/pages/logs.tsx).
- Toolbar layout, filter affordance design, and breakpoint behavior.
- Tests in [dashboard/client/src/pages/__tests__/logs.test.tsx](dashboard/client/src/pages/__tests__/logs.test.tsx).
- Optional, only if a clean primitive falls out: a small addition to [dashboard/client/src/components/ui/dashboard-primitives.tsx](dashboard/client/src/components/ui/dashboard-primitives.tsx). Do not invent a Toolbar primitive just to feel productive; keep the layout page-local if that is simpler.

### Scope (out)

- No backend, no agent, no dashboard server changes.
- No log-source taxonomy changes.
- No persistence schema changes (`buba.logs.preferences.v1` localStorage key stays).
- No favicon or icon work (Run 4).
- No new pages.

### Acceptance

- Desktop (>= 1280 px wide): one intentional console toolbar. Search, severity, source, event-type, Follow, Wrap, line-count laid out so they read as one row or two clearly grouped rows. Never overlap or wrap unintentionally.
- iPad landscape (~ 1024 px): same intent as desktop but tighter. May fold to two clearly grouped rows. No overlap, no clipped labels.
- iPad portrait (~ 768 px): two-row toolbar acceptable. Search and core filters visible without expansion. Utility toggles (Follow, Wrap, line count) compactly grouped, not floating.
- Phone (320 px to 480 px): compact filter affordance. Not a giant `Filters` button. Acceptable forms: icon + short-label disclosure button (e.g. lucide `SlidersHorizontal`), segmented compact control, or small inline disclosure row that matches dashboard density.
- All breakpoints: no overlap, no clipped labels, no controls drifting across borders, no orphan single-control rows.
- Keyboard navigation, focus rings, and ARIA labels preserved on every control.
- Tap targets at phone width remain usable (aim >= 32 px effective hit target where possible without breaking the dense aesthetic).
- The localStorage preference round-trip continues to work for every existing field (`lines`, `follow`, `wrap`, `search`, `severity`, `source`, `eventType`).
- Tests cover: existing preference persistence (already exists, must keep passing), filter behavior (already exists), and new responsive affordance presence at relevant breakpoints where practical (presence of compact mobile button, absence of giant `Filters (...)` text, hidden vs visible select clusters).

### Files

- [dashboard/client/src/pages/logs.tsx](dashboard/client/src/pages/logs.tsx)
- [dashboard/client/src/pages/__tests__/logs.test.tsx](dashboard/client/src/pages/__tests__/logs.test.tsx)
- [dashboard/client/src/components/ui/dashboard-primitives.tsx](dashboard/client/src/components/ui/dashboard-primitives.tsx) (only if a primitive emerges naturally)

Reload the file before editing because it has uncommitted changes (already verified at plan time).

### Gates

```bash
cd dashboard/client
./node_modules/.bin/tsc --noEmit -p tsconfig.app.json
npm test -- --run
npm run lint
npm run build
```

Visual verification at desktop, narrow desktop, iPad portrait, iPad landscape, and phone widths through dev server + browser. Take a screenshot at each breakpoint.

### Notes for plan mode

- The /frontend-design skill is not the right tool for this run. Match the existing dashboard aesthetic; this is a tactical responsive fix, not a creative redesign.
- Container queries (`@container`) may be cleaner than viewport queries since the toolbar lives in a single container.
- Look at how Trades, Signals, and Equity pages handle their toolbars before inventing a new pattern, in case they already settled a convention worth reusing.

---

## Run 2: Runtime Config page (full stack)

### Why

The owner repeatedly asks "which strategy params and config knobs are currently running?" Today this is reconstructable only by SSHing into the host and reading `docker-compose.live-readonly.yml`, `.env`, and tailing logs. This is friction, error-prone, and creates ambiguity about whether the deployed values match what is documented. A read-only Monitor page should answer this question without SSH.

### Scope (in)

- A sanitized runtime-config snapshot persisted by the bot at startup. Preferred storage: a single `runtime_config_snapshot` JSON value in `run_metadata` (already used for `runtime_capture_health`, `replay_quality_class`, etc.). Alternative if cleaner: extend `live_sessions.details_json` for the latest session row.
- A read-only agent endpoint, suggested `GET /api/runtime/config`, returning the parsed snapshot plus minimal observed runtime context (process start time, uptime).
- A dashboard server proxy at `GET /api/bots/:id/config`.
- Client typed hook (e.g. `useRuntimeConfig(botId)`) and a new page at `/config`, label `Config`, section `Monitor`, scope `operations`, registered in [dashboard/client/src/lib/routes.ts](dashboard/client/src/lib/routes.ts).
- Rust tests for the new agent and dashboard-server surfaces.
- Frontend tests for the new hook and page.
- Doc update describing the new page and its data sources.

### Scope (out)

- No mutation controls. Read-only only.
- No new strategy params or config knobs. This run only surfaces existing ones.
- No log-tailing or env-file reading from the dashboard. The snapshot is the only source.
- No live cron-style polling at the bot. Snapshot is written once at startup; agent serves it directly.

### Snapshot fields (sanitized)

The snapshot must include at least the following groups. Never include private keys, relayer secrets, agent/dashboard JWT secrets, sidecar credentials, or any value that could compromise auth.

- Identity: execution mode (`paper` / `live_readonly` / `live_trading`), runtime name if available, process start time (ms), DB path label if safe to expose, config fingerprint (already `live_sessions.config_fingerprint`), git SHA / release tag if available at build.
- Storage and replay: `FEED_EVENT_STORAGE_PROFILE`, configured replay capability, observed runtime capture health (from `run_metadata.runtime_capture_health`), recent feed classes, DB growth cap.
- Strategy toggles: `LATENCY_ARB_ENABLED`, `SPREAD_CAPTURE_ENABLED`, `CALM_PERSISTENCE_ENABLED`.
- Latency-arb params: momentum threshold, min/max ask, cooldown, adaptive window, max position fraction.
- Spread params (always exposed, even when disabled): threshold, leg skew, quote churn, max position fraction.
- Calm params (always exposed, even when disabled): window times, max ask, distance/vol/churn/alignment/fair-bias/edge/max-position knobs.
- Risk and bankroll: starting balance, max position fraction, min bet, max drawdown, live cash cap, live max single order, live max open notional, live daily loss, live session drawdown.
- Pending settlement: family reserve fraction, global reserve fraction, counts-as-open-position.
- Fees and paper assumptions: taker fee rate, taker fee exponent, simulated order latency.
- Feed freshness: max feed age, max quote age, CLOB no-message reconnect threshold, Binance no-message reconnect threshold.
- Worker budgets: feed writer queue/batch/flush/lag, decision queue/output capacities, runtime persistence queue, submission queue, max live decision age, shutdown timeout.

### Frontend presentation

- Group values by category (Identity, Storage and Replay, Strategies, per-strategy panels, Risk, Pending Settlement, Fees and Paper Assumptions, Feed Freshness, Worker Budgets).
- Strategy enabled/disabled state must be visually unmistakable.
- Make the canary baseline obvious where applicable (e.g. "Run 012 latency-only profile" if matched).
- Highlight safety-critical values (live cash cap, live daily loss, live session drawdown).
- Show config fingerprint prominently and copyable.
- Distinguish "configured value" (from snapshot) from "observed runtime state" (from current process / agent reads). Two visually distinct columns or labels.
- Match the existing dashboard primitive language (SectionCard, KeyValueList, StatusChip, CopyButton). Do not invent a new aesthetic.

### Files

- [bots/paint/src/live.rs](bots/paint/src/live.rs) and/or [bots/paint/src/live_readonly.rs](bots/paint/src/live_readonly.rs): persist sanitized snapshot at startup.
- [bots/paint/src/config.rs](bots/paint/src/config.rs): sanitized export helpers if not already present.
- [bots/paint/src/db/database.rs](bots/paint/src/db/database.rs): use `set_run_metadata` already in place.
- [agent/src/api.rs](agent/src/api.rs), [agent/src/db_reader.rs](agent/src/db_reader.rs), [agent/src/types.rs](agent/src/types.rs), [agent/src/main.rs](agent/src/main.rs).
- [dashboard/server/src/main.rs](dashboard/server/src/main.rs), [dashboard/server/src/api/bots.rs](dashboard/server/src/api/bots.rs), [dashboard/server/src/proxy.rs](dashboard/server/src/proxy.rs).
- [dashboard/client/src/lib/api.ts](dashboard/client/src/lib/api.ts), [dashboard/client/src/lib/types.ts](dashboard/client/src/lib/types.ts), [dashboard/client/src/lib/routes.ts](dashboard/client/src/lib/routes.ts).
- [dashboard/client/src/hooks/](dashboard/client/src/hooks/): new `use-runtime-config.ts`.
- [dashboard/client/src/pages/](dashboard/client/src/pages/): new `config.tsx`.
- [dashboard/client/src/App.tsx](dashboard/client/src/App.tsx): wire the route.
- [dashboard/client/src/components/layout/nav.tsx](dashboard/client/src/components/layout/nav.tsx) and [dashboard/client/src/components/layout/app-shell.tsx](dashboard/client/src/components/layout/app-shell.tsx): nav item.
- Doc updates in [docs/system-architecture.md](docs/system-architecture.md) and [docs/commands-and-config.md](docs/commands-and-config.md) where relevant.

### Acceptance

- `GET /api/bots/<id>/config` returns the sanitized snapshot for the deployed `live_readonly` runtime, plus observed identity (process start time, uptime).
- The new `/config` page renders all groups, marks strategy enabled/disabled clearly, exposes the config fingerprint with copy, and never shows secrets.
- Rust tests cover the new agent endpoint behavior, including missing-snapshot fallback.
- Frontend tests cover hook loading/error/data and key page assertions (group headings present, fingerprint copyable, enabled flag rendered).
- The bot does not change its decision behavior. The new persistence is one additive write at startup.

### Gates

Backend:

```bash
cargo build
cargo test -p buba-paint
cargo test -p buba-agent
cargo test -p buba-dashboard
make lint
make comment-audit
make docs-audit
git diff --check
```

Frontend:

```bash
cd dashboard/client
./node_modules/.bin/tsc --noEmit -p tsconfig.app.json
npm test -- --run
npm run lint
npm run build
```

### Notes for plan mode

- Optionally invoke /frontend-design only for the visual-hierarchy thinking on the page; do not let it drift away from the existing dashboard aesthetic. The page should match Execution / Overview density, not invent a new one.
- The "Run 012 latency-only profile" canary label is a presentation-layer match (compare snapshot fingerprint or strategy mix vs. a known constant), not bot-side state.

---

## Run 3: Machine/System Health page (full stack)

### Why

The owner repeatedly asks "is the server healthy?" — disk, RAM, CPU, swap, DB/WAL growth, and network. Today this is answered by SSH and ad-hoc commands. The page must answer it cheaply, bounded, and without putting the bot at risk.

### Scope (in)

- A read-only agent endpoint, suggested `GET /api/system`, returning a bounded JSON of host/runtime health.
- A dashboard server proxy at `GET /api/bots/:id/system`.
- Client typed hook (e.g. `useSystemHealth(botId)`) and a new page at `/system` (label `System`, section `Monitor`, scope `operations`).
- Sampling state: the agent stores an in-memory "previous CPU sample" so that subsequent calls return a delta. Bounded; no unbounded history.
- Doc updates describing the new page and its data sources.

### Scope (out)

- No Docker socket access, no `docker exec` into other containers from the agent. Agent stays in its sandbox.
- No host-level metrics that require root or extra privileges. Use `/proc` and the runtime bind mount.
- No SQLite `quick_check`, no replay validators, no whole-table scans. File sizes only.
- No mutation controls. Read-only only.

### Allowed data sources

- Runtime filesystem stats for `/runtime`: total/free/used (disk).
- Runtime file sizes: `paint.db`, `paint.db-wal`, `paint.db-shm`, log files.
- DB row counts only if bounded or already cached. Prefer file sizes and writer metadata over scans.
- `/proc/meminfo`: RAM available, swap total/used.
- `/proc/loadavg`: 1m/5m/15m load averages.
- `/proc/stat`: CPU usage and iowait delta, computed against an in-memory previous sample held by the agent.
- `/proc/net/dev`: RX/TX counters by interface.
- Existing agent process status and dashboard health endpoints.
- `run_metadata` keys already present (capture health, replay quality, dropped/error counts) read through the existing DB pattern.

### Docker caveat (must be acknowledged in plan and UI)

The agent runs inside a container. `/proc` reads may surface containerized values rather than full host truth. Runtime disk stats on `/runtime` are still useful because `/runtime` is a host bind mount. Where a value is best-effort (CPU iowait, network), label it as such in the UI ("container-scoped") rather than misrepresenting it as host truth.

### UI cards and warning thresholds

Top status strip: `Healthy` / `Warning` / `Critical`, derived from the worst card.

Cards:

- Disk: free GB, used %, runtime directory size, DB/WAL/SHM sizes, projected time to disk pressure if a growth rate is known.
- CPU: current CPU %, load average (1m/5m/15m), iowait. Mark container-scoped if applicable.
- Memory: available RAM %, swap used %.
- Runtime DB: `paint.db`, WAL, SHM, log sizes, DB growth cap, recent feed classes (from `run_metadata`).
- Services: bot, sidecar, agent, dashboard, Caddy health from existing checks where available.
- Capture: runtime capture health, feed classes present, dropped rows, writer errors, queue full count.
- Warnings panel: each active warning with the exact action text the operator should take.

Thresholds:

- Disk critical: free `< 2 GiB` OR used `> 90%`.
- Disk warning: free `< 5 GiB` OR used `> 80%`.
- WAL warning: WAL larger than DB by a large factor for a sustained period, or growing fast.
- Runtime DB warning: projected 24h growth exceeds remaining safe disk.
- RAM warning: available `< 20%`.
- Swap warning: used `> 25%`.
- CPU warning: sustained `> 70%` or iowait `> 5%`.
- Capture critical: runtime capture health not `sweep_grade` in a live-readonly/research run, OR writer errors, OR dropped rows, OR queue full, OR terminal write errors.
- Service critical: bot/agent/dashboard/sidecar unhealthy or unexpectedly restarted.

### Frontend presentation

- Operator console feel, not generic cloud dashboard. Match the dashboard's existing monospace + dense aesthetic.
- Cards with terse labels, tabular numbers, and inline status chips.
- Avoid huge tables. Compact sparklines may help if cheap to compute, but are optional.
- Make values copyable where useful (paths, sizes).
- Warnings have explicit action text ("Free disk before 2 GiB"; "Restart sidecar via partial redeploy"). No generic "Warning" with no follow-up.

### Files

- [agent/src/api.rs](agent/src/api.rs), [agent/src/main.rs](agent/src/main.rs), [agent/src/types.rs](agent/src/types.rs).
- New agent module(s) for `/proc`/FS reads and CPU sampling state.
- [dashboard/server/src/main.rs](dashboard/server/src/main.rs), [dashboard/server/src/api/bots.rs](dashboard/server/src/api/bots.rs), [dashboard/server/src/proxy.rs](dashboard/server/src/proxy.rs).
- [dashboard/client/src/lib/api.ts](dashboard/client/src/lib/api.ts), [dashboard/client/src/lib/types.ts](dashboard/client/src/lib/types.ts), [dashboard/client/src/lib/routes.ts](dashboard/client/src/lib/routes.ts).
- [dashboard/client/src/hooks/](dashboard/client/src/hooks/): new `use-system-health.ts`.
- [dashboard/client/src/pages/](dashboard/client/src/pages/): new `system.tsx`.
- [dashboard/client/src/App.tsx](dashboard/client/src/App.tsx), nav, app-shell.
- Docs: [docs/deployment-and-ops.md](docs/deployment-and-ops.md), [docs/system-architecture.md](docs/system-architecture.md).

### Acceptance

- `GET /api/bots/<id>/system` returns a bounded JSON, completes well under one second, never scans the DB.
- The page renders all six cards with the deployed `live_readonly` runtime values, plus a status strip and warnings panel.
- Container-scoped values are labeled as such where applicable.
- The page never shows secrets and never executes `docker exec` or `quick_check`.
- Rust tests cover normal and degraded paths (file missing, `/proc` parse failure, etc.).
- Frontend tests cover hook loading/error/data and warning state rendering.

### Gates

Same as Run 2.

### Notes for plan mode

- /frontend-design can be useful for thinking about visual hierarchy of the cards and warning prominence, but the page must remain inside the dashboard aesthetic.
- Sketch the JSON shape in the plan first; the API shape drives both the agent and the page.
- Consider using a single short-poll interval (e.g. 5 s) on the page rather than a WebSocket. The endpoint is cheap to recompute.

---

## Run 4: Favicon and App Icon pipeline (frontend assets)

### Why

The favicon/app-icon work has been fought several times and is still inconsistent. Past attempts caused: Chrome/Firefox/Safari desktop favicons drifted apart; Safari sometimes showed nothing, sometimes a grey placeholder, sometimes a giant fake white border; some generated rasters were pixelated, monochrome, or lost the arrow shape; iOS home-screen icons had inner-border alignment problems. The goal is a deliberate pipeline, not raster guessing.

### Scope (in)

- One canonical vector source matching [dashboard/client/src/components/layout/logo.tsx](dashboard/client/src/components/layout/logo.tsx) geometry. The current logo is the green checkmark/arrow on a dark rounded square with a white border (per the existing `favicon.svg`).
- A reproducible asset-generation pipeline (script in [scripts/](scripts/) or a package script) so future updates are not manual raster guessing.
- Per-platform asset set:
  - Browser favicon SVG (`favicon.svg`).
  - Browser favicon raster fallbacks (`favicon-16x16.png`, `favicon-32x32.png`, `favicon-48x48.png`, `favicon-64x64.png`).
  - `favicon.ico` multi-size (16/32/48).
  - Safari pinned-tab `mask-icon.svg` as monochrome path.
  - Apple touch icon at 180x180, opaque, high resolution, no accidental transparency.
  - Android `icon-192x192.png` and `icon-512x512.png` (purpose `any`), plus `icon-192x192-maskable.png` and `icon-512x512-maskable.png` with safe-zone-compliant artwork.
- Updated [dashboard/client/index.html](dashboard/client/index.html) and [dashboard/client/public/site.webmanifest](dashboard/client/public/site.webmanifest) only as needed to point at the regenerated assets.
- Strengthened tests in [dashboard/client/src/lib/__tests__/pwa-assets.test.ts](dashboard/client/src/lib/__tests__/pwa-assets.test.ts).

### Scope (out)

- No new branding. Match the existing logo geometry.
- No backend.
- No additional manifest categories.
- No PWA service-worker rework beyond what icons require.
- No runtime UI changes.

### Past failures (do not re-introduce)

- Quick raster overwrites that pixelated the arrow.
- Apple touch icon shipped with transparency; iOS rendered it with an unwanted dark backing.
- Safari `mask-icon` `color` attribute mismatched with the rest of the UI palette.
- Phantom white borders appearing in Safari due to background-detection behavior; the chosen white border must be intentional and stable across browsers, not relied-on as a Safari side effect.
- iOS PWA icon caching: deleting and re-adding the home-screen app is mandatory when verifying iOS changes; Safari aggressively caches app icons.

### Required research before plan mode

- Safari favicon and pinned-tab behavior, including the `mask-icon` color semantics and any current quirks with SVG favicons in Safari.
- iOS PWA icon caching rules and the standard delete-and-re-add verification dance.
- Android maskable icon safe-zone (40% inner radius) and how Chrome renders both `any` and `maskable` purposes.
- Whether the "white rounded border" the dashboard uses today should be encoded in the SVG itself (recommended) or relied on as a platform effect (do not rely).
- Up-to-date best practice for SVG favicon support in current Chrome and Firefox (the existing `favicon.svg` already targets this).

### Pipeline shape (suggested)

- Single source: `assets/icons/source.svg` (or similar), authoritative.
- Generator script (e.g. `scripts/build-icons.mjs`) using `sharp`, `svg2png`, `inkscape`, or similar, that produces the entire asset set with correct dimensions, opacity, and safe zones.
- Output writes into [dashboard/client/public/](dashboard/client/public/).
- The generated artifacts are committed; the script is documented in the README block at the top of the script and in [docs/deployment-and-ops.md](docs/deployment-and-ops.md) so future updates do not regress.

### Per-platform requirements

- Chrome / Firefox tab: SVG favicon must look crisp and match the UI logo. PNG fallbacks for older browsers (16/32/48/64) sharp at all sizes.
- Safari tab: must NOT show a double border, fake giant border, missing icon, or black square. The intentional border is part of the SVG, not a platform effect.
- iOS home screen: `apple-touch-icon.png` at 180x180, opaque, high-res, no transparency. Inner geometry safe in iOS rendering (no clipping at corners).
- Android home screen: `icon-192/512.png` for `purpose=any` and `icon-192/512-maskable.png` for `purpose=maskable`; the maskable artwork keeps the arrow inside the safe zone for circular masks.

### Acceptance

- All existing assertions in [pwa-assets.test.ts](dashboard/client/src/lib/__tests__/pwa-assets.test.ts) continue to pass.
- New assertions cover: `favicon.ico` exists and has at least three sizes; `mask-icon.svg` is monochrome; maskable icons keep their content inside a safe zone; Apple touch icon is opaque (already partially asserted).
- Manual verification (not automatable inline): tab favicon crisp in Chrome, Firefox, and desktop Safari; iOS home-screen icon correct after delete-and-re-add; Android home-screen icon correct in both `any` and `maskable` contexts.
- The generator script is reproducible: re-running it on a clean checkout produces byte-identical outputs (allowing for known nondeterminism in PNG metadata; if so, that nondeterminism must be documented or stripped).

### Files

- New: `scripts/build-icons.mjs` (or equivalent), `assets/icons/source.svg` (or equivalent path).
- Updated: every file in [dashboard/client/public/](dashboard/client/public/) that holds an icon asset.
- Updated: [dashboard/client/index.html](dashboard/client/index.html), [dashboard/client/public/site.webmanifest](dashboard/client/public/site.webmanifest).
- Updated: [dashboard/client/src/lib/__tests__/pwa-assets.test.ts](dashboard/client/src/lib/__tests__/pwa-assets.test.ts).
- Optional: [docs/deployment-and-ops.md](docs/deployment-and-ops.md) for pipeline docs.

### Gates

```bash
cd dashboard/client
./node_modules/.bin/tsc --noEmit -p tsconfig.app.json
npm test -- --run
npm run build
```

Plus a clean-tree run of the icon-generator script to confirm reproducibility.

### Notes for plan mode

- /frontend-design is not the right tool for this run. The geometry is fixed by the existing logo; the work is platform-specific asset pipeline, not creative design.
- Web research is required before plan-mode finalization (Safari quirks, iOS cache, Android maskable safe zones). Cite sources in the plan.

---

## Final closeout (after Run 4 lands)

- Update [docs/system-architecture.md](docs/system-architecture.md) and [docs/deployment-and-ops.md](docs/deployment-and-ops.md) with a brief description of the new Monitor pages and their data sources, only with durable facts (not implementation history).
- Add a concise final note (in the relevant doc, not in PROMPT.md) explaining what data is live truth, what is best-effort, and what cannot be observed safely without stronger host integration. Examples:
  - Live truth: snapshot config fingerprint, runtime DB file sizes, capture health key in `run_metadata`.
  - Best-effort: container-scoped CPU iowait, network counters in containerized `/proc`, projected disk-pressure ETA.
  - Not observed: full host metrics requiring Docker socket or root; intentionally not implemented for safety.
- Delete this `PROMPT.md` once all four runs ship and the durable docs are updated. Do not let it linger as a stale plan.
