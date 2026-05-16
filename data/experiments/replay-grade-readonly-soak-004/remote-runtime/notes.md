# Replay-Grade Readonly Soak 004: Remote Runtime Logs

Non-secret evidence copied back from the Finland host (`buba-paint-fin`) after the May 2 readonly-soak attempt. See [../notes.md](../notes.md) for the soak summary and [../manifest.json](../manifest.json) for the soak script result.

## Files

* `agent.log`: stdout/stderr from the supervised `buba-agent` user systemd unit.
* `dashboard.log`: stdout/stderr from the supervised `buba-dashboard` user systemd unit.
* `paint.log`: stdout/stderr from the supervised `buba-paint-bot` user systemd unit running in `EXECUTION_MODE=live_readonly`.
* `sidecar.log`: stdout/stderr from the supervised `buba-polymarket-sidecar` user systemd unit (Phase 9 May-2 instance, before the auth path stabilized).
* `env-report.json`: redacted Polymarket env summary the sidecar emitted at startup. No secrets, only key presence/length and configured non-secret flags.

The corresponding remote runtime DB at `/root/buba-paint-live/runtime/soak-004-20260502-152805Z/paint.db` was deliberately not copied back. Source-of-truth runtime DBs live on the host.
