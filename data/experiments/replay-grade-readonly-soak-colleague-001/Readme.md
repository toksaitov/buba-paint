# Replay-Grade Readonly Soak: Colleague 001

Independent no-order `live_readonly` soak attempt against `buba-paint-fin` per the former root `PROMPT.md` handoff. Scope: paper trading plus authenticated CLOB v2 readonly account/order reads. No `live_trading`, no arming, no orders, no cancels, no redemptions.

## Status

In progress. Auth path independently re-verified after the May 2 Phase 9 401/Cloudflare blocker. See [probes/notes.md](probes/notes.md) for run-by-run probe evidence and timings.

The soak itself (5-minute and 90-minute runs) had not started at the time this directory was created. When the soak completes, the `live-readiness-host-soak.py` script writes the deployment manifest, per-step logs, `live-preflight.json`, sidecar/agent/dashboard health snapshots, and the local copy of the remote runtime evidence into this directory.

## Layout

- `probes/`: independent CLOB v2 auth probe artifacts and per-host run notes (Ireland and, when SSH allows, Finland).
- Future: `manifest.json`, numbered step logs, `live-preflight.json`, `remote-runtime/` evidence copied back from the Finland host.

## Safety guarantees

- No live ledger writes: `live_order_intents`, `live_orders`, `live_fills`, `live_redemptions` must remain zero throughout.
- No secret values are persisted in this directory; all probe outputs run through the redaction pass before they are copied back.
- Remote runtime DB stays on the host; we copy text logs and validation summaries only.
