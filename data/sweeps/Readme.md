# Sweeps

This directory holds derived parameter sweep outputs.

Preferred naming:

- `run-XXX-topic-NNN`
- example: `run-012-001`

Every new sweep directory should include either `notes.md`, `RUNNING.md`, or `SWEEP_BLOCKED.md` explaining the input DB, git SHA, command, and conclusion.

Sweeps are now blocked unless the selected input interval is `sweep_grade`. Use `buba-paint validate-replay-data --data <db> --start <time> --end <time>` before launching a long sweep.

Historical directories with older names such as `rust-010` and `calm-004` are retained as legacy research outputs.
