# Scripts

`scripts/` contains repo maintenance, setup, and manual analysis utilities.

Root-level scripts are active automation or setup entry points. Manual charting helpers live under `scripts/analysis/`. Historical one-off scripts live under `scripts/archive/`.

## Active Automation

- `audit-docs.py`: checks repo-owned `Readme.md` casing, local Markdown links, `data/` directory notes, stale file references, active-plan files under `docs/`, root scratch files, and transient derived SQLite WAL/SHM files. Run through `make docs-audit`.
- `check_coverage.py`: enforces component coverage floors for `make coverage-gate`.
- `ts_comment_audit.mjs`: enforces the TypeScript comment policy used by `make lint` and `make comment-audit`.

## Setup

- `setup-ubuntu.sh`: installs host dependencies and builds the Rust workspace, dashboard client, and Polymarket sidecar on an Ubuntu host.

## Manual Analysis

Use `scripts/analysis/` for quick legacy-compatible charts against an explicit run DB path. These helpers are not replay-grade sweep tooling and do not validate data quality.

## Archive

Use `scripts/archive/` for run-specific or old one-off scripts kept only for provenance. Archived scripts should not be treated as current workflow templates.

Do not write temporary DBs, logs, or scratch outputs into this directory. Use `/tmp` for throwaway work or `data/experiments/run-XXX-topic-NNN` for durable derived analysis.
