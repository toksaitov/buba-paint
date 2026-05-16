# Run 014 latency-only sweep

Status: running
Started: 2026-05-12T14:20:57Z
Git SHA: b3c1896, with local uncommitted worktree changes present
PID: 11127

Input archive:

* Raw DB: `runs/014/server-20260511-171249/paint.db`
* Prepared DB: `data/sweeps/run-014-latency-001/prepared.db`
* Interval: `2026-05-11T17:25:00Z` to `2026-05-12T12:00:00Z`

Validation completed before sweep:

* `validate-replay-data`: `sweep_grade`
* `prepare-backtest-input`: `prepared_backtest=ready`, `backtest_input=backtest_ready`
* Baseline latency-only replay: 59,767,459 ticks, 223 windows, 3 trades, PnL `$26.07`

Sweep scope:

* Latency arbitrage only.
* Spread capture disabled.
* Calm persistence disabled.
* Grid: `8 x 5 x 5 x 3 = 600` combinations.
* Threads: `RAYON_NUM_THREADS=8`.
* Starting balance `$100`.
* Pending settlement mode uses observed market resolution and risky run profile: `0.0 / 0.25 / false`.

Output:

* CSV: `data/sweeps/run-014-latency-001/sweep.csv`
* Log: `data/sweeps/run-014-latency-001/sweep.log`

Check commands:

```bash
tail -n 80 data/sweeps/run-014-latency-001/sweep.log
grep -E "\\[[0-9]+/[0-9]+\\]|ERROR|Sweep complete|Results:" data/sweeps/run-014-latency-001/sweep.log | tail -n 40
pgrep -fl "buba-paint sweep"
df -h .
```
