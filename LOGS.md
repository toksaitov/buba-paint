# Soak Log Locations

Where to read logs and evidence for the in-flight `live_readonly` soak on `buba-paint-fin`.

## 1. Local evidence

5-minute run (complete, passed acceptance): [data/experiments/replay-grade-readonly-soak-colleague-001/](data/experiments/replay-grade-readonly-soak-colleague-001/)

- [manifest.json](data/experiments/replay-grade-readonly-soak-colleague-001/manifest.json): final verdict, command list, release/runtime paths.
- [notes.md](data/experiments/replay-grade-readonly-soak-colleague-001/notes.md): script-generated soak summary.
- [Readme.md](data/experiments/replay-grade-readonly-soak-colleague-001/Readme.md): pre-soak context.
- [deliverables.md](data/experiments/replay-grade-readonly-soak-colleague-001/deliverables.md): full handoff.
- [live-preflight.json](data/experiments/replay-grade-readonly-soak-colleague-001/live-preflight.json), [replay-quality.txt](data/experiments/replay-grade-readonly-soak-colleague-001/replay-quality.txt), [env-report.json](data/experiments/replay-grade-readonly-soak-colleague-001/env-report.json).
- 23 numbered step logs `01-...log` through `23-...log` (one per soak step).
- [probes/](data/experiments/replay-grade-readonly-soak-colleague-001/probes/): auth probe artifacts and [probe-report.md](data/experiments/replay-grade-readonly-soak-colleague-001/probes/probe-report.md).

90-minute run (in progress, populates as the run proceeds): [data/experiments/replay-grade-readonly-soak-colleague-002/](data/experiments/replay-grade-readonly-soak-colleague-002/). The same artifact set will land here when the run completes.

Quickest at-a-glance status:

```bash
ls -lat data/experiments/replay-grade-readonly-soak-colleague-002/ | head -10
```

The newest file's mtime tells you which step just wrote. Step name is in the filename (`12-poll-01-health.log`, `16-validate-replay-data.log`, `17-remote-acceptance-check.log`, etc.).

Local readiness gate (last green run): `/private/tmp/buba-live-readiness-local-20260503-054205Z/manifest.json` plus 14 numbered logs in the same directory.

## 2. Live remote logs on `buba-paint-fin`

Most useful while the soak is running:

```bash
ssh buba-paint-fin 'tail -f /root/buba-paint-live/runtime/soak-001-colleague-20260503-060213Z/sidecar.log'
ssh buba-paint-fin 'tail -f /root/buba-paint-live/runtime/soak-001-colleague-20260503-060213Z/paint.log'
ssh buba-paint-fin 'journalctl --user -u buba-polymarket-sidecar.service -f --no-pager'
ssh buba-paint-fin 'journalctl --user -u buba-paint-bot.service -f --no-pager'
ssh buba-paint-fin 'journalctl --user -u buba-agent.service -f --no-pager'
ssh buba-paint-fin 'journalctl --user -u buba-dashboard.service -f --no-pager'
```

After closeout the runtime directory stays at `/root/buba-paint-live/runtime/soak-001-colleague-20260503-060213Z/` (preserved per PROMPT.md). The 5-minute run's runtime is at `/root/buba-paint-live/runtime/soak-001-colleague-20260503-054927Z/`.

## 3. Background-task tail (the `make` driving the soak)

The full `make live-readiness-host-soak` stdout/stderr goes to:

`/private/tmp/claude-501/-Users-toksaitov-Desktop-buba-paint/982e66b2-64a9-4c04-b723-28413c373530/tasks/b5rqmzvnn.output`

`tail -200` produces output only after the make returns; while the soak runs that file stays small (around 300 bytes from the early `stamp=...` and `python3 scripts/...` echoes). The final verdict line appears there when the soak finishes.
