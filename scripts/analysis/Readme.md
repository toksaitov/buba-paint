# Analysis Scripts

This directory contains manual plotting helpers for inspecting run databases.

These scripts are still useful for quick charts because current run DBs retain the legacy-compatible tables they read, including `tick_data`, `markets`, `signals`, `simulated_trades`, `trade_results`, and `balance_log`. They are not replay-grade research tooling and they do not inspect newer raw `feed_events`, live-readonly tables, or replay-quality metadata.

Use them for fast visual checks, not parameter selection.

```bash
python3 scripts/analysis/chart-run.py              runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/pnl_curve.py              runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/latency_distribution.py   runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/spread_over_time.py       runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/signal_frequency.py       runs/012/server-20260424-183503/paint.db
python3 scripts/analysis/binance_vs_chainlink.py   runs/012/server-20260424-183503/paint.db
```

Each helper requires an explicit DB path. Most helpers write a PNG into the current working directory; `chart-run.py` writes into the DB directory.

