# buba-paint

Paper-trading system for Polymarket 5-minute BTC Up/Down prediction markets.
Connects to three live WebSocket feeds, detects latency-arbitrage and
spread-capture opportunities, simulates trades with bankroll-aware
position sizing (per-strategy half-Kelly criterion), and logs everything
to SQLite. Built in Rust for low-latency execution and fast backtesting
(275-combo parameter sweep in ~42 seconds via rayon parallelism).
569 tests, 91.4% line coverage.

No real orders, no wallet, no private keys. This is a data-collection
and strategy-validation tool.

## Quick Start

```bash
cargo build --release              # optimized binary
cargo test                         # 569 tests
cargo clippy -- -D warnings        # lint (zero warnings required)
cargo run --release -- live --db-path runs/008/buba-paint.db --balance 200
```

Requires Rust 1.94+ (install via [rustup](https://rustup.rs)).

## How It Works

Every 5 minutes, Polymarket opens a market: "Will BTC go Up or Down?"
The bot exploits two edges:

**Latency Arb** -- Binance spot price moves faster than the Polymarket
CLOB reprices. When Binance shows strong momentum but CLOB odds are
stale, the bot logs a simulated directional buy. Features adaptive
momentum threshold, 60s cooldown, min/max ask filters, and confidence
scaling.

**Spread Capture** -- When UP ask + DOWN ask < $1.00, the market is
mispriced. The bot buys both sides for guaranteed profit at settlement.
Rejects entries where either side is below $0.15.

At window close, positions settle binary: winning side pays $1, losing
pays $0. P&L computed under four fee assumptions (0-3%).

### Three Feeds

| Feed             | Source                                                  | Data                                     | Rate             |
| ---------------- | ------------------------------------------------------- | ---------------------------------------- | ---------------- |
| Binance aggTrade | `wss://stream.binance.com:9443/ws/btcusdt@aggTrade`     | Per-trade BTC/USDT price                 | ~20-100 msg/s    |
| Polymarket CLOB  | `wss://ws-subscriptions-clob.polymarket.com/ws/market`  | Order book snapshots + price changes     | Variable         |
| Chainlink RTDS   | `wss://ws-live-data.polymarket.com`                     | Oracle BTC/USD price (settlement ref)    | ~1 msg/s         |

All feeds auto-reconnect with exponential backoff (1s base, 30s max, jitter).
Chainlink detects silent staleness and force-reconnects.

## Architecture

Single Rust binary with three subcommands: `live`, `backtest`, `sweep`.
Built with tokio (async I/O) and rayon (parallel backtesting).

| Module                 | Responsibility                                                |
| ---------------------- | ------------------------------------------------------------- |
| `cli.rs`               | CLI parsing (clap), command dispatch                          |
| `live.rs`              | Live trading loop: feeds + discovery + strategies + settle    |
| `bankroll.rs`          | Per-strategy half-Kelly sizing, confidence curve, DD pause    |
| `position_manager.rs`  | Trade lifecycle, opposing position guard, settlement          |
| `circuit_breaker.rs`   | Pause after consecutive losses                                |
| `market_discovery.rs`  | Gamma API polling, window lifecycle                           |
| `tick_logger.rs`       | 1s interval tick sampling to SQLite                           |
| `feeds/`               | Three WebSocket feeds with auto-reconnect                     |
| `strategies/`          | Latency-arb + spread-capture (Strategy trait)                 |
| `backtest/`            | Tick replay engine, parameter sweep (rayon), window manager   |
| `db/`                  | SQLite wrapper (WAL mode), schema, `build-data` merge tool    |
| `config.rs`            | All env-configurable settings                                 |

See `CLAUDE.md` for the full module map and AI development guidelines.

## Project Structure

```
buba-paint/
  Cargo.toml                       # Rust project config
  CLAUDE.md                        # AI development guidelines
  Readme.md                        # This file
  rustfmt.toml                     # Formatter config
  src/
    main.rs                        # Entry point (delegates to cli.rs)
    lib.rs                         # Module exports
    cli.rs                         # CLI parsing + command dispatch
    live.rs                        # Live paper trading orchestrator
    config.rs                      # All env-configurable settings
    types.rs                       # Shared data structures
    clock.rs                       # Injectable clock (System + Backtest)
    errors.rs                      # Error types
    bankroll.rs                    # Kelly sizing, confidence curve, DD pause
    position_manager.rs            # Trade lifecycle + settlement
    circuit_breaker.rs             # Consecutive loss pause
    trend_tracker.rs               # Directional trend filter (experimental)
    market_discovery.rs            # Gamma API polling, window events
    tick_logger.rs                 # 1s tick sampling to SQLite
    feeds/                         # WebSocket feed modules
      binance_feed.rs              #   Binance aggTrade stream
      chainlink_feed.rs            #   Chainlink oracle + staleness
      clob_feed.rs                 #   CLOB order book + resubscription
      util.rs                      #   Backoff with jitter
    strategies/                    # Trading strategy modules
      latency_arb.rs               #   Momentum vs stale odds
      spread_capture.rs            #   UP+DOWN ask < threshold
    backtest/                      # Replay engine + sweep
      runner.rs                    #   Core replay loop
      sweep.rs                     #   Parallel parameter sweep (rayon)
      tick_replay.rs               #   Tick grouping (10ms tolerance)
      window_manager.rs            #   Market window replay
      feed_state.rs                #   Simulated feed state
      momentum.rs                  #   Rolling momentum calculator
    db/                            # SQLite persistence
      database.rs                  #   rusqlite wrapper, WAL mode
      schema.rs                    #   6 tables with indexes
      build_data.rs                #   Merge run DBs → market-data.db
    tests/                         # Unit tests (via #[path])
    */tests/                       # Unit tests for submodules
  tests/                           # Integration tests
    backtest_test.rs               #   Backtest replay verification
    sweep_test.rs                  #   Sweep CSV + determinism
    feeds_test.rs                  #   Feed tests (mock WebSocket)
    discovery_test.rs              #   Discovery tests (mock HTTP)
    live_system_test.rs            #   Full E2E system test
    cli_test.rs                    #   CLI command dispatch
    build_data_test.rs             #   Data merge end-to-end
    support/mock_ws.rs             #   Mock WebSocket server
  legacy-ts/                       # Archived TypeScript implementation
  scripts/                         # Python analysis scripts
  data/                            # Derived data (reproducible)
    market-data.db                 #   Merged ticks from all runs (LFS)
    sweeps/                        #   Parameter sweep results
    experiments/                   #   Walk-forward validation DBs
  runs/                            # Primary live data (IRREPLACEABLE)
    001/ ... 008/                  #   DB, logs, analysis PNGs (LFS)
```

**Data preservation:** `runs/` contains primary data collected during live
paper trading sessions. This data is irreplaceable -- never delete or modify
files in `runs/`. `data/` is derived and reproducible.

## CLI Reference

### Live paper trading

```bash
cargo run --release -- live --db-path runs/008/buba-paint.db --balance 200
cargo run --release -- live --set LATENCY_ARB_MAX_ASK=0.55
```

### Single backtest

```bash
cargo run --release -- backtest \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 --set PEAK_DD_PAUSE_PCT=1.0
```

### Parameter sweep

```bash
cargo run --release -- sweep \
  --data data/market-data.db \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.001:0.003:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.45,0.50,0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125,0.15 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50 --set PEAK_DD_PAUSE_PCT=1.0 \
  --output data/sweeps/rust-005/sweep.csv
```

`--sweep PARAM=start:end:step` generates a range; `--sweep PARAM=a,b,c` enumerates.
`--set PARAM=value` fixes a parameter without sweeping.

### Build merged market data

```bash
cargo run --release -- build-data                          # default: runs/ → data/market-data.db
cargo run --release -- build-data --runs-dir runs --output data/market-data.db
```

Merges tick data, markets, and trade results from all run DBs into a single
database for backtesting. Source DBs opened read-only (`?mode=ro`). Idempotent.

## Configuration

All settings via environment variables or `--set` CLI flag.

### Core

| Variable              | Default                | Description                              |
| --------------------- | ---------------------- | ---------------------------------------- |
| `DB_PATH`             | `./data/buba-paint.db` | SQLite database path                     |
| `LOG_LEVEL`           | `info`                 | debug, info, warn, error                 |
| `TICK_INTERVAL`       | `1000`                 | Tick sampling interval (ms)              |
| `GAMMA_POLL_INTERVAL` | `60000`                | Gamma API poll interval (ms)             |
| `CHAINLINK_STALE_MS`  | `30000`                | Force-reconnect after silence (ms)       |

### Latency Arb

| Variable                          | Default  | Description                       |
| --------------------------------- | -------- | --------------------------------- |
| `LATENCY_ARB_MOMENTUM_THRESHOLD`  | `0.0015` | Min momentum fraction (0.15%)     |
| `LATENCY_ARB_MAX_ASK`             | `0.55`   | Max ask to consider stale         |
| `LATENCY_ARB_MIN_ASK`             | `0.30`   | Min ask (rejects cheap tokens)    |
| `LATENCY_ARB_COOLDOWN_MS`         | `60000`  | Cooldown between signals (ms)     |
| `MOMENTUM_WINDOW_MS`              | `30000`  | Momentum rolling window (ms)      |

### Spread Capture

| Variable                    | Default | Description                     |
| --------------------------- | ------- | ------------------------------- |
| `SPREAD_CAPTURE_THRESHOLD`  | `0.998` | Max UP+DOWN ask sum             |
| `SPREAD_CAPTURE_MIN_ASK`    | `0.15`  | Reject degenerate books below   |

### Bankroll

| Variable                      | Default | Description                     |
| ----------------------------- | ------- | ------------------------------- |
| `STARTING_BALANCE`            | `150`   | Initial paper balance (USD)     |
| `MAX_POSITION_FRACTION`       | `0.10`  | Max fraction per trade (10%)    |
| `MAX_POSITION_USD_FRACTION`   | `0.20`  | Hard cap per trade (20%)        |
| `MIN_BALANCE_THRESHOLD`       | `20`    | Stop trading below this         |
| `MAX_DRAWDOWN_PCT`            | `0.50`  | Stop at 50% drawdown            |

### Kelly Criterion

| Variable                 | Default | Description                            |
| ------------------------ | ------- | -------------------------------------- |
| `KELLY_FRACTION`         | `0.5`   | Kelly multiplier (half-Kelly)          |
| `MIN_WIN_RATE_FOR_KELLY` | `0.52`  | Min win rate to apply Kelly            |
| `MIN_TRADES_FOR_KELLY`   | `20`    | Fixed fraction until this many trades  |
| `KELLY_ROLLING_WINDOW`   | `30`    | Rolling window per strategy            |
| `MIN_KELLY_FLOOR`        | `0.03`  | Min fraction floor (3%)                |
| `MIN_BET_USD`            | `5`     | Min bet size (USD)                     |

### Position Limits & Safety

| Variable                   | Default   | Description                             |
| -------------------------- | --------- | --------------------------------------- |
| `MAX_OPEN_POSITIONS`       | `5`       | Max concurrent positions                |
| `MIN_WINDOW_TIME_MS`       | `90000`   | Don't enter with < 90s left             |
| `CIRCUIT_BREAKER_LOSSES`   | `3`       | Pause after N consecutive losses        |
| `CIRCUIT_BREAKER_PAUSE_MS` | `900000`  | Pause duration (15 min)                 |
| `PEAK_DD_PAUSE_PCT`        | `0.30`    | Pause at 30% drawdown from peak         |
| `PEAK_DD_PAUSE_MS`         | `3600000` | DD pause duration (1 hour)              |
| `DD_PAUSE_RECOVERY_PCT`    | `0.05`    | DD must recover by 5% before re-arming  |
| `RECONNECT_MIN_STABLE_MS`  | `5000`    | Min connection duration to reset backoff|
| `RECONNECT_MAX_FAILURES`   | `20`      | Feed circuit breaker threshold          |
| `RECONNECT_PAUSE_MS`       | `300000`  | Feed circuit breaker pause (5 min)      |

### Trend Filter (experimental, off by default)

| Variable                  | Default | Description                          |
| ------------------------- | ------- | ------------------------------------ |
| `TREND_FILTER_ENABLED`    | `false` | Enable counter-trend suppression     |
| `TREND_FILTER_THRESHOLD`  | `0.30`  | Bias threshold to suppress           |
| `TREND_FILTER_WINDOW`     | `10`    | Recent outcomes to consider          |

## Database Schema

SQLite (WAL mode). Python scripts can read concurrently while the bot writes.

### tick_data -- 1s sampled from all feeds

| Column     | Type    | Description                                |
| ---------- | ------- | ------------------------------------------ |
| timestamp  | INTEGER | Unix ms                                    |
| source     | TEXT    | binance, clob_up, clob_down, chainlink     |
| price      | REAL    | Spot price (binance/chainlink)             |
| bid        | REAL    | Best bid (clob rows)                       |
| ask        | REAL    | Best ask (clob rows)                       |
| bid_size   | REAL    | Size at best bid                           |
| ask_size   | REAL    | Size at best ask                           |

### markets -- one row per 5-minute window

| Column          | Type    | Description                              |
| --------------- | ------- | ---------------------------------------- |
| market_id       | TEXT    | Gamma API ID (unique)                    |
| question        | TEXT    | Market question                          |
| condition_id    | TEXT    | On-chain condition                       |
| slug            | TEXT    | URL slug                                 |
| up_token_id     | TEXT    | CLOB token ID for UP outcome             |
| down_token_id   | TEXT    | CLOB token ID for DOWN outcome           |
| start_time      | INTEGER | Window start (Unix ms)                   |
| end_time        | INTEGER | Window end (Unix ms)                     |
| status          | TEXT    | active, closed, resolved                 |

### signals, simulated_trades, trade_results, balance_log

See `src/db/schema.rs` for full DDL. Key relationships:
`signals` (every detection) -> `simulated_trades` (opened positions) ->
`trade_results` (settlement P&L) + `balance_log` (balance history).

## Backtesting

The tick-level backtester replays historical data through the real strategy
code. Ticks are loaded into memory, then replayed through strategies,
bankroll, and settlement -- identical to the live path.

### Sweep Results

| Sweep      | Runtime | Best PnL | Notes                                              |
| ---------- | ------- | -------- | -------------------------------------------------- |
| 001 (TS)   | ~40 min | $14,602  | INVALID -- stale temp DBs inflated balance         |
| 002 (TS)   | ~40 min | $1,282   | v0.5 peak DD pause throttled at $200 start         |
| 003 (TS)   | ~40 min | $4,070   | Baseline -- DD pause disabled, matches live        |
| rust-004   | 42s     | $4,070   | Rust parity validation (identical to 003)          |
| rust-005   | 42s     | $4,070   | Post-refactor -- identical to rust-004             |
| rust-006   | 4 min   | $815,678 | Full range (Feb 15 -- Mar 20, 8.8M ticks)          |

Always use `--set PEAK_DD_PAUSE_PCT=1.0` and `--set SPREAD_CAPTURE_THRESHOLD=0.50`
in sweeps. The DD pause is too aggressive for small starting balances,
and spread-capture overcounts ~18x due to 1s tick sampling.

## Run History

| Run | Duration | Trades | Win Rate | P&L (0%)  | Notes                                    |
| --- | -------- | ------ | -------- | --------- | ---------------------------------------- |
| 001 | --       | --     | --       | --        | v0.0, first test run                     |
| 002 | 5h       | 9      | 55.6%    | +$69      | v0.1, fixed 100-token bets               |
| 003 | 1h       | 1      | 0%       | -$11      | v0.2, bankroll-aware sizing              |
| 004 | 96h      | 76     | 51.3%    | +$719     | v0.2, $200->$919, peak $1,556            |
| 005 | 25h      | 11     | 36.4%    | -$5       | v0.3, over-filtering bug                 |
| 006 | 267h     | 292    | 56.5%    | +$4,565   | v0.4, $200->$4,765, peak $9,678          |
| 007 | 222h     | 187    | 56.7%    | +$5,488   | v0.5 TS, $200→$5,688, peak $8,130        |
| 008 | ongoing  | --     | --       | --        | v0.6 Rust, $200 start, deployed Mar 20   |

## Analysis Scripts

```bash
python3 scripts/pnl_curve.py              runs/006/buba-paint.db
python3 scripts/latency_distribution.py   runs/006/buba-paint.db
python3 scripts/spread_over_time.py       runs/006/buba-paint.db
python3 scripts/signal_frequency.py       runs/006/buba-paint.db
python3 scripts/binance_vs_chainlink.py   runs/006/buba-paint.db
```

Requires Python 3 with matplotlib, pandas, numpy.
Each script produces a `.png` and an interactive matplotlib window.

## Development

```bash
cargo test                                           # 569 tests (517 unit + 52 integration)
cargo clippy -- -D warnings                          # pedantic linting
cargo fmt                                            # auto-format
cargo llvm-cov --all-targets --summary-only          # 91.4% line coverage
```

Unit tests live in `src/*/tests/` directories (via `#[path]` attribute).
Integration tests in `tests/` use mock WebSocket servers and wiremock
for HTTP. See `CLAUDE.md` for testing practices and architecture rules.

## Deployment

```bash
# Single binary, no runtime dependencies
cargo build --release
scp target/release/buba-paint buba-paint:~/bot/

# On server (Ubuntu 24.04, ARM64, t4g.micro in eu-west-1)
./buba-paint live --db-path runs/008/buba-paint.db --balance 200
```

The bot runs indefinitely, rolling through 5-minute windows. Ctrl+C for
graceful shutdown (final stats printed, feeds disconnect, DB closes).
