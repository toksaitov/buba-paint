# Run Index

`runs/` contains primary evidence from live paper and readonly sessions. Do not edit these DBs or logs manually. The only supported mutation is an explicit additive migration workflow such as `upgrade-history`.

Current local run sequence:

- `runs/001` through `runs/009`: early live paper runs and screenshots.
- `runs/010`: pulled run-018 live-paper artifacts used for parity work.
- `runs/011`: local and server run 011 artifacts.
- `runs/012`: archived readonly shadow run from April 2026. This was originally discussed and archived from the server as run 013, then renamed locally to keep numbering contiguous. The DB and logs were not edited.

Run 012 status:

- primary archive: `runs/012/server-20260424-183503`
- derived forensics: `data/experiments/run-012-forensics-001`
- blocked sweep note: `data/sweeps/run-012-001/SWEEP_BLOCKED.md`
- quality: descriptive only, not sweep-grade

Run 012 remains valuable for realized PnL, drawdown chain, halt behavior, strategy attribution, and operational health. It must not be used for trusted parameter selection because the original compact capture omitted Binance `bookTicker` rows required to reconstruct live decision state.
