# buba-paint v0.5

Paper-trading system for Polymarket 5-minute BTC Up/Down prediction markets.
Connects to three live WebSocket feeds, detects latency-arbitrage and
spread-capture opportunities, simulates trades with bankroll-aware position
sizing (per-strategy half-Kelly criterion), logs everything to SQLite, and
produces analysis visualizations.

No real orders, no wallet, no private keys. This is a data-collection and
strategy-validation tool.

## How It Works

Every 5 minutes, Polymarket opens a new market asking "Will BTC go Up or Down
in the next 5 minutes?" resolved by a Chainlink oracle. The bot exploits two
potential edges:

**Strategy A -- Latency Arb.** Binance BTC spot price moves faster than the
Polymarket CLOB order book reprices. When Binance shows strong momentum
(configurable threshold, default 0.15% over 30 seconds) but the CLOB still
shows stale odds, the bot logs a simulated directional buy at the current best
ask. Features a 60-second cooldown between signals, a minimum entry price
filter (rejects tokens below $0.30 which historically lost 100%), adaptive
momentum threshold (85th percentile of recent momentum, ensures signals only
fire on unusually strong moves), and confidence scoring that scales position
size by momentum strength.

**Strategy B -- Spread Capture.** When the sum of the best ask for the UP token
and the best ask for the DOWN token falls below $1.00 (configurable threshold,
default $0.998), the market is theoretically mispriced. The bot buys BOTH sides
simultaneously (with balanced equal-token sizing) for a guaranteed profit at
settlement regardless of outcome. Rejects entries where either side is below
$0.15 to avoid extreme tail bets. This strategy fired 13 times in run 004 with
+$484 P&L, proving that brief spread collapses do occur.

At each 5-minute window close, open positions are resolved using the Chainlink
settlement price, and P&L is computed under four fee assumptions (0%, 1%, 2%,
3%).

## Architecture

The system is a single Node.js process with these components:

- `main.ts` -- orchestrator that wires everything together, handles startup and
  graceful shutdown. Evaluates strategies on every Binance tick and CLOB book
  update (throttled to 200ms minimum interval).
- Three WebSocket feeds run concurrently, each with auto-reconnect:
    - `binance-feed.ts` reads Binance aggTrade, computes rolling momentum.
    - `clob-feed.ts` reads Polymarket CLOB, maintains top-of-book for UP/DOWN.
    - `chainlink-feed.ts` reads Polymarket RTDS, tracks Chainlink oracle price.
      Detects stale feeds (no data for 30s) and auto-reconnects.
- `market-discovery.ts` polls the Gamma API every 60s to find the active
  5-minute BTC window, emits `newWindow` / `windowClosed` events.
- On `newWindow`, `clob-feed` resubscribes with the new token IDs.
- On each Binance tick and CLOB book update, two strategies evaluate the state:
    - `latency-arb.ts` -- fires when momentum is high but CLOB odds are stale.
    - `spread-capture.ts` -- fires when UP ask + DOWN ask < threshold.
- `bankroll-manager.ts` -- tracks balance, sizes positions using per-strategy
  half-Kelly criterion (after 20 trades; fixed fraction before that) with a
  rolling-window win rate (last 30 trades per strategy). Enforces a hard position
  size cap (20% of balance), steeper confidence curve (signals below 0.55
  confidence get zero size), drawdown limit, and minimum balance threshold.
  Balanced spread-capture sizing ensures both legs get equal token counts.
  Persists balance to SQLite for recovery on restart.
- `position-manager.ts` opens simulated trades from signals (using
  bankroll-aware sizing), blocks opposing positions from the same strategy in
  the same window (while allowing batch signals like spread-capture to open
  both sides atomically via `tryOpenSpread()`), resolves trades at window close
  using Chainlink price, computes P&L at 4 fee levels.
- `circuit-breaker.ts` -- pauses all trading for 15 minutes after 3 consecutive
  losses, preventing runaway loss streaks.
- `trend-tracker.ts` -- experimental (off by default) filter that suppresses
  counter-trend signals based on rolling win rates by direction.
- `regime-detector.ts` -- experimental (off by default) market regime classifier.
  Tracks 1-minute return reversals over a 2-hour window; classifies the market as
  trending, choppy, or unknown. When enabled and regime is choppy, suppresses
  latency-arb signals.
- `tick-logger.ts` samples all feeds every 1s into the `tick_data` table.
- All data is persisted to a SQLite database (WAL mode).

Data flow: Binance/CLOB/Chainlink feeds --> strategies evaluate -->
signals logged --> bankroll sizes position --> trade opened --> window closes -->
positions resolved --> bankroll updated --> P&L recorded. Tick logger runs
independently on a 1-second timer.

### Three Live Feeds

| Feed                      | URL                                                    | Data                                                  | Rate             |
| ------------------------- | ------------------------------------------------------ | ----------------------------------------------------- | ---------------- |
| Binance aggTrade          | `wss://stream.binance.com:9443/ws/btcusdt@aggTrade`    | Per-trade BTC/USDT price and quantity                 | ~20-100 msgs/sec |
| Polymarket CLOB           | `wss://ws-subscriptions-clob.polymarket.com/ws/market` | Order book snapshots and price changes for UP/DOWN    | Variable         |
| Polymarket RTDS Chainlink | `wss://ws-live-data.polymarket.com`                    | Chainlink BTC/USD oracle price (settlement reference) | ~1 msg/sec       |

All feeds auto-reconnect with exponential backoff (1s base, 30s max, with
jitter). The Chainlink feed also detects silent data loss (WebSocket open but
no updates for `CHAINLINK_STALE_MS`) and force-reconnects; during staleness
`getPrice()` returns null so the bot falls back to Binance.

### Market Discovery

Markets are accessed via the Gamma API events endpoint using a predictable slug:

```
GET https://gamma-api.polymarket.com/events/slug/btc-updown-5m-{UNIX_TIMESTAMP}
```

where `UNIX_TIMESTAMP` is `floor(now_seconds / 300) * 300`. The response
contains a `markets` array with `clobTokenIds` (the UP and DOWN token IDs needed
for CLOB subscription), `conditionId`, `endDate`, and `outcomes`
(`["Up", "Down"]`).

The generic `/markets` endpoint does NOT reliably return these short-lived
5-minute markets. The slug-based event lookup is the only reliable discovery
method.

### Bankroll Management

The `BankrollManager` controls all position sizing. Key mechanics:

- **Starting balance**: configurable (default $150), recoverable from DB on
  restart.
- **Position sizing**: each trade uses at most `MAX_POSITION_FRACTION` (10%) of
  current balance AND at most `MAX_POSITION_USD_FRACTION` (20%) as a hard cap.
- **Per-strategy Kelly criterion**: after `MIN_TRADES_FOR_KELLY` (10) trades,
  switches from fixed fraction to half-Kelly (`f* = (bp - q) / b` scaled by
  0.5), computed using each strategy's own win rate (not a global average).
  Requires observed win rate > 52% to bet.
- **Rolling window win rate**: uses the last `KELLY_ROLLING_WINDOW` (30) trades
  per strategy for faster adaptation when edge decays, falling back to lifetime
  stats when rolling data is insufficient.
- **Steeper confidence curve**: `multiplier = max(0, (confidence - 0.5) * 2.5)`.
  Signals at 0.50 confidence get zero size (filtered out), 0.60 → 25% of Kelly,
  0.90 → 100%. This naturally filters weak signals without changing strategy
  logic.
- **Balanced spread sizing**: spread-capture pairs are sized together via
  `reserveSpreadCapital()`, ensuring both legs get equal token counts based on
  total pair cost.
- **Circuit breaker**: after `CIRCUIT_BREAKER_LOSSES` (3) consecutive losses,
  all trading pauses for `CIRCUIT_BREAKER_PAUSE_MS` (15 minutes / 3 windows).
- **Peak drawdown pause**: if balance drops 30% below the all-time high water
  mark, all trading pauses for 1 hour to prevent cascading losses at large
  bet sizes.
- **Safety limits**: stops trading if balance drops below $20 or drawdown
  exceeds 50%.
- **Balance persistence**: every balance change is logged to the `balance_log`
  table. On restart, the bot recovers the last known balance.

## Project Structure

```
buba-paint/
  package.json              # ESM project, tsx scripts
  tsconfig.json             # Strict, ES2022, bundler resolution
  .env.example              # All configurable thresholds with docs
  src/
    main.ts                 # Orchestrator: startup, event wiring, shutdown
    config.ts               # All constants from env vars with defaults
    types.ts                # Shared TypeScript interfaces
    clock.ts                # Injectable clock (Date.now for live, manual for backtest)
    bankroll-manager.ts     # Per-strategy Kelly sizing, position caps, rolling WR
    position-manager.ts     # Trade open/resolve, spread sizing, opposing guard
    circuit-breaker.ts      # Consecutive loss detection, trading pause
    market-discovery.ts     # Gamma API event lookup by slug, window lifecycle
    tick-logger.ts          # 1s interval sampling all feeds to SQLite
    trend-tracker.ts        # Experimental directional trend filter
    regime-detector.ts      # Experimental market regime classifier
    db/
      schema.ts             # 6 SQLite tables with indexes
      database.ts           # better-sqlite3 wrapper, prepared statements
    feeds/
      base-feed.ts          # Abstract WebSocket base: connect, reconnect, ping
      binance-feed.ts       # aggTrade parser, rolling momentum window
      clob-feed.ts          # Book snapshots + price_change, top-of-book state
      chainlink-feed.ts     # RTDS subscribe, staleness detection, auto-reconnect
    strategies/
      latency-arb.ts        # Momentum vs stale odds, adaptive threshold, cooldown
      spread-capture.ts     # UP ask + DOWN ask < threshold, buys both sides
    backtest/
      run.ts                # CLI: single backtest run
      runner.ts             # Core replay loop: ticks → strategies → trades
      sweep.ts              # CLI: parameter sweep with tick caching
      tick-replay.ts        # Reads tick_data, groups by 10ms tolerance
      window-manager.ts     # Replays market windows from DB
      feed-state.ts         # Simulated feed state for strategies
      momentum.ts           # Offline momentum calculator
    data/
      build-market-db.ts    # Merges per-run DBs into market-data.db
    utils/
      logger.ts             # Timestamped structured logger with level filter
  scripts/                    # Python analysis + infra
    chart-run.py              # 6-panel dashboard + trade detail chart
    pnl_curve.py, spread_over_time.py, latency_distribution.py
    signal_frequency.py, binance_vs_chainlink.py
    setup-ubuntu.sh           # AWS/Ubuntu 24.04 setup
    run-006/                  # Run-specific deep analysis
      chart-analysis.py, chart-deep.py, deep_analysis.py
  data/                       # Derived data (computed, reproducible)
    market-data.db            # Merged tick data from all runs (Git LFS)
    sweeps/                   # Parameter sweep campaign results
      001/sweep.csv           # INVALID — stale DB contamination
      001/notes.md
      002/sweep.csv           # Valid, but throttled by peak DD pause
      002/notes.md
      003/sweep.csv           # Baseline — DD pause disabled, matches live
      003/notes.md
    experiments/              # Validation & walk-forward results
      validate-006/           # Backtester accuracy validation
      wf-split-a/             # Walk-forward: train Feb 20-28, test Feb 28-Mar 4
      wf-split-b/             # Walk-forward: test on run 007 data
  runs/                       # Primary data — collected live (Git LFS)
    001/ ... 007/             # Each: buba-paint.db, bot.log, analysis PNGs
```

## Setup

### Local Development

```bash
# Install Node dependencies
npm install

# Copy and optionally edit config
cp .env.example .env

# Verify TypeScript compiles
npm run typecheck
```

Requires Node.js >= 18 (for native `fetch`). Python 3 with `matplotlib`,
`pandas`, and `numpy` for analysis scripts.

### AWS Deployment (Ubuntu 24.04, ARM64)

```bash
# Rsync project to server (assumes SSH config with host "buba-paint")
rsync -avz --exclude node_modules --exclude data --exclude runs \
  --exclude '*.db' --exclude .git ./ buba-paint:~/buba-paint/

# Run setup on server
ssh buba-paint 'bash -s' < scripts/setup-ubuntu.sh
```

The setup script installs Node.js, npm, sqlite3, Python 3, matplotlib, pandas,
numpy, then runs npm install and typecheck.

**Recommended instance**: `t4g.micro` in eu-west-1 (Ireland). The bot uses
~135 MB RAM, <1% CPU, ~145 KB/s bandwidth. Ireland is the closest
non-geoblocked region to Polymarket's eu-west-2 (London) backend, giving
~2ms TCP latency.

## Running the Bot

### Development (auto-restart on changes)

```bash
npm run dev
```

### Production (single run)

```bash
npm start
```

### With custom config

```bash
LOG_LEVEL=debug STARTING_BALANCE=200 npm start
```

### Long unattended run on AWS

```bash
# Indefinite run, output to named run directory
mkdir -p runs/004
STARTING_BALANCE=200 DB_PATH=runs/004/buba-paint.db \
  nohup npx tsx src/main.ts > runs/004/bot.log 2>&1 &

# Check on it
tail -20 runs/004/bot.log
ps aux | grep tsx
```

The bot runs indefinitely, rolling through 5-minute windows.
Press Ctrl+C for graceful shutdown (final bankroll stats printed, all feeds
disconnect, DB closes cleanly).

### What to Expect on Startup

```
[INFO] [main]      === buba-paint paper trading bot v0.5 ===
[INFO] [main]      Config: momentum=0.0015, window=30000ms, cooldown=60000ms,
                   min_ask=0.3, balance=$150, max_fraction=10%, max_pos_usd=20%,
                   kelly=0.5, circuit_breaker=3L/15min
[INFO] [db]        Database initialized at runs/004/buba-paint.db
[INFO] [bankroll]  Initialized with $150.00
[INFO] [chainlink] Connected
[INFO] [chainlink] Initial data dump: 59 entries, latest $69032.12
[INFO] [discovery] New market window: Bitcoin Up or Down - Feb 15, 10:50-10:55 ET
[INFO] [clob]      Subscribing to market {...token IDs...}
[INFO] [binance]   Connected
[INFO] [main]      All systems running.
```

When strategies fire:

```
[INFO] [main]      SIGNAL: latency-arb => UP | confidence=0.65 |
                   momentum=0.2500% | UP ask=0.510 DOWN ask=0.490
[INFO] [bankroll]  Reserved $11.22 (22 tokens @ $0.510) |
                   fraction=7.5% confidence=0.65 | balance=$150.00
[INFO] [positions] TRADE OPENED #1: latency-arb UP @ 0.510 (22 tokens, $11.22)
...
[INFO] [positions] TRADE RESOLVED #1: WIN latency-arb UP |
                   entry=0.510 settlement=1 |
                   P&L(0%)=$10.78 P&L(3%)=$10.44
[INFO] [bankroll]  Trade #1 settled: WIN $10.78 | balance=$160.78 |
                   W/L=1/0 (100%) | drawdown=0.0%
```

On window close:

```
[INFO] [main]      BANKROLL: $160.78 | P&L=$10.78 | W/L=1/0 (100%) |
                   drawdown=0.0%
```

## Database Schema

SQLite database (WAL mode, safe for concurrent reads from Python).

### tick_data

Sampled every `TICK_INTERVAL` ms (default 1 second) from all feeds.

| Column    | Type    | Description                               |
| --------- | ------- | ----------------------------------------- |
| timestamp | INTEGER | Unix ms                                   |
| source    | TEXT    | binance, clob_up, clob_down, or chainlink |
| price     | REAL    | Spot price (binance, chainlink rows only)  |
| bid       | REAL    | Best bid (clob_up, clob_down rows only)    |
| ask       | REAL    | Best ask (clob_up, clob_down rows only)    |
| bid_size  | REAL    | Size at best bid                           |
| ask_size  | REAL    | Size at best ask                           |

### markets

One row per 5-minute window the bot observed.

| Column        | Type    | Description                                   |
| ------------- | ------- | --------------------------------------------- |
| market_id     | TEXT    | Gamma API market ID (unique)                  |
| question      | TEXT    | e.g. "Bitcoin Up or Down - Feb 14, 7:55-8 ET" |
| condition_id  | TEXT    | On-chain condition ID                         |
| slug          | TEXT    | e.g. btc-updown-5m-1771116900                 |
| up_token_id   | TEXT    | CLOB token ID for the UP outcome              |
| down_token_id | TEXT    | CLOB token ID for the DOWN outcome            |
| start_time    | INTEGER | Window start, Unix ms                         |
| end_time      | INTEGER | Window end, Unix ms                           |
| status        | TEXT    | active, closed, or resolved                   |

### signals

Every strategy detection, whether or not a trade was opened.

| Column          | Type    | Description                            |
| --------------- | ------- | -------------------------------------- |
| timestamp       | INTEGER | Unix ms                                |
| strategy        | TEXT    | latency-arb or spread-capture          |
| direction       | TEXT    | UP or DOWN                             |
| binance_price   | REAL    | Binance BTC price at signal time       |
| chainlink_price | REAL    | Chainlink oracle price at signal time  |
| up_ask          | REAL    | CLOB UP token best ask                 |
| down_ask        | REAL    | CLOB DOWN token best ask               |
| up_bid          | REAL    | CLOB UP token best bid                 |
| down_bid        | REAL    | CLOB DOWN token best bid               |
| metadata        | TEXT    | JSON with strategy-specific details    |

### simulated_trades

Opened when a signal passes position manager guards and bankroll approval.

| Column      | Type    | Description                             |
| ----------- | ------- | --------------------------------------- |
| timestamp   | INTEGER | Unix ms                                 |
| market_id   | TEXT    | FK to markets                           |
| strategy    | TEXT    | Which strategy opened this              |
| side        | TEXT    | UP or DOWN                              |
| token_id    | TEXT    | Which CLOB token was "bought"           |
| entry_price | REAL    | Best ask at entry                       |
| size        | REAL    | Token count (bankroll-sized, not fixed) |
| status      | TEXT    | open, closed, or expired                |

### trade_results

One row per resolved trade, joined to simulated_trades on trade_id.

| Column           | Type    | Description                           |
| ---------------- | ------- | ------------------------------------- |
| trade_id         | INTEGER | FK to simulated_trades                |
| exit_price       | REAL    | 1.0 (win) or 0.0 (loss)              |
| settlement_price | REAL    | Same as exit_price for binary markets |
| pnl_0pct         | REAL    | (settlement - entry) * size           |
| pnl_1pct         | REAL    | pnl_0pct minus 1% of entry cost      |
| pnl_2pct         | REAL    | pnl_0pct minus 2% of entry cost      |
| pnl_3pct         | REAL    | pnl_0pct minus 3% of entry cost      |
| resolved_at      | INTEGER | Unix ms when resolved                 |

### balance_log

Tracks every balance change for recovery on restart and post-run analysis.

| Column    | Type    | Description                                |
| --------- | ------- | ------------------------------------------ |
| timestamp | INTEGER | Unix ms                                    |
| event     | TEXT    | init, trade_open, or trade_close           |
| trade_id  | INTEGER | FK to simulated_trades (null for init)     |
| amount    | REAL    | Change amount (0 for init, P&L for close)  |
| balance   | REAL    | Balance after this event                   |

## Configuration Reference

All settings via environment variables. Defaults shown.

### Core

| Variable            | Default              | Description                           |
| ------------------- | -------------------- | ------------------------------------- |
| DB_PATH             | ./data/buba-paint.db | SQLite database file path             |
| LOG_LEVEL           | info                 | debug, info, warn, or error           |
| TICK_INTERVAL       | 1000                 | Tick sampling interval in ms          |
| GAMMA_POLL_INTERVAL | 60000                | How often to re-check Gamma API in ms |
| CHAINLINK_STALE_MS  | 30000                | Force-reconnect after this many ms without data |

### Strategy A: Latency Arb

| Variable                       | Default | Description                                  |
| ------------------------------ | ------- | -------------------------------------------- |
| LATENCY_ARB_MOMENTUM_THRESHOLD | 0.0015  | Min Binance momentum fraction (0.0015 = 0.15%) |
| LATENCY_ARB_MAX_ASK            | 0.55    | Max CLOB ask to consider "stale" odds        |
| LATENCY_ARB_MIN_ASK            | 0.30    | Min ask to enter (rejects cheap tokens)      |
| LATENCY_ARB_COOLDOWN_MS        | 60000   | Cooldown between signals in ms               |
| MOMENTUM_WINDOW_MS             | 30000   | Rolling window for momentum calc in ms       |

### Strategy B: Spread Capture

| Variable                  | Default | Description                                  |
| ------------------------- | ------- | -------------------------------------------- |
| SPREAD_CAPTURE_THRESHOLD  | 0.998   | Max UP+DOWN ask sum to fire spread capture   |
| SPREAD_CAPTURE_MIN_ASK    | 0.15    | Reject degenerate book sides below this      |

### Bankroll Management

| Variable                 | Default | Description                                   |
| ------------------------ | ------- | --------------------------------------------- |
| STARTING_BALANCE         | 150     | Initial paper balance in USD                  |
| MAX_POSITION_FRACTION    | 0.10    | Max fraction of balance per trade (10%)       |
| MAX_POSITION_USD_FRACTION| 0.20    | Hard cap: no single trade > 20% of balance    |
| MIN_BALANCE_THRESHOLD    | 20      | Stop trading below this balance               |
| MAX_DRAWDOWN_PCT         | 0.50    | Stop trading at 50% drawdown from peak        |

### Kelly Criterion

| Variable               | Default | Description                                  |
| ---------------------- | ------- | -------------------------------------------- |
| KELLY_FRACTION         | 0.5     | Kelly multiplier (0.5 = half-Kelly)          |
| MIN_WIN_RATE_FOR_KELLY | 0.52    | Min observed win rate to apply Kelly         |
| MIN_TRADES_FOR_KELLY   | 20      | Use fixed fraction until this many per-strategy trades |
| KELLY_ROLLING_WINDOW   | 30      | Use last N trades per strategy for win rate  |
| MIN_KELLY_FLOOR        | 0.03    | Min fraction when Kelly says 0 (3% floor)    |
| MIN_BET_USD            | 5       | Min bet size in USD (prevents dust bets)     |

### Position Limits

| Variable           | Default | Description                                   |
| ------------------ | ------- | --------------------------------------------- |
| MAX_OPEN_POSITIONS | 5       | Max concurrent simulated positions            |
| MIN_WINDOW_TIME_MS | 90000   | Don't enter trades with less than 90s left    |

### Circuit Breaker

| Variable                | Default  | Description                                 |
| ----------------------- | -------- | ------------------------------------------- |
| CIRCUIT_BREAKER_LOSSES  | 3        | Pause after this many consecutive losses    |
| CIRCUIT_BREAKER_PAUSE_MS| 900000   | Pause duration in ms (15 minutes)           |

### Peak Drawdown Pause

| Variable                | Default  | Description                                 |
| ----------------------- | -------- | ------------------------------------------- |
| PEAK_DD_PAUSE_PCT       | 0.30     | Pause when balance drops 30% from peak      |
| PEAK_DD_PAUSE_MS        | 3600000  | Pause duration in ms (1 hour)               |

### Trend Filter (experimental, off by default)

| Variable                | Default | Description                                 |
| ----------------------- | ------- | ------------------------------------------- |
| TREND_FILTER_ENABLED    | false   | Enable counter-trend signal suppression     |
| TREND_FILTER_THRESHOLD  | 0.30    | Directional bias threshold to suppress      |
| TREND_FILTER_WINDOW     | 10      | Number of recent outcomes to consider       |

### Regime Detection (experimental, off by default)

| Variable                   | Default | Description                                 |
| -------------------------- | ------- | ------------------------------------------- |
| REGIME_DETECTION_ENABLED   | false   | Enable market regime classification         |

## Analysis Scripts

All scripts accept an optional DB path argument (default `data/buba-paint.db`)
and produce both a `.png` file and an interactive matplotlib window.

```bash
# From the project root, or from a laptop after copying the .db file
python3 scripts/latency_distribution.py [path/to/buba-paint.db]
python3 scripts/spread_over_time.py
python3 scripts/pnl_curve.py
python3 scripts/signal_frequency.py
python3 scripts/binance_vs_chainlink.py
```

### latency_distribution.py

Measures the core question: how long does it take the CLOB to react after a
Binance price move? Joins `tick_data` rows where `source='binance'` with the
next `source IN ('clob_up','clob_down')` row within 15 seconds. Outputs a
histogram and CDF of the delay in milliseconds. Also shows CLOB update
frequency distribution.

Key metrics printed: mean, median, P95, P99 delay.
Output file: `latency_distribution.png`.

### spread_over_time.py

Exact-timestamp join of UP and DOWN asks from `tick_data`. Per-window bar chart
showing minimum combined ask for each 5-minute window, with threshold line.
Shows how close the market gets to spread-capture territory.

Key metrics printed: percentage of samples below $1.00 and below threshold.
Output file: `spread_over_time.png`.

### pnl_curve.py

Per-trade P&L bar chart (for small trade counts) or cumulative P&L line (for
larger datasets). Shows results across all four fee levels. Bottom chart:
per-strategy breakdown at 0% fee.

Key metrics printed: total trades, win rate, total and average P&L per strategy.
Output file: `pnl_curve.png`.

### signal_frequency.py

BTC price timeline with signal markers overlaid (for small signal counts) or
hourly frequency bar chart (for larger datasets). Shows when each strategy
fired relative to price action.

Key metrics printed: total signals per strategy, signals/hour rate.
Output file: `signal_frequency.png`.

### binance_vs_chainlink.py

Price comparison between Binance spot and Chainlink oracle, resampled to 30s
intervals. Rolling mean delta with sigma bands. KDE histogram of delta
distribution. Stats box with mean, std, max lag.

Key metrics printed: mean, std, max, min delta.
Output file: `binance_vs_chainlink.png`.

## Backtesting

A tick-level backtesting engine replays historical tick data (~6M+ rows) through
the real strategy code. This enables fast iteration without waiting for live
runs.

### Merged Market Database

Run data from individual runs is merged into a single `data/market-data.db` via:

```bash
npm run build-data
```

This copies `tick_data` and `markets` tables from each run DB into a unified
database, deduplicating overlapping timestamps.

### Single Backtest

```bash
npm run backtest -- \
  --data data/market-data.db \
  --out data/experiments/test.db \
  --start 2026-02-20T03:13 --end 2026-03-04T04:26 \
  --balance 200
```

Replays ticks through latency-arb and spread-capture strategies with full
bankroll management, circuit breaker, and Kelly sizing. Results are written to a
SQLite database compatible with the analysis scripts.

### Parameter Sweep

```bash
npm run sweep -- \
  --data data/market-data.db \
  --output data/sweeps/002/sweep.csv \
  --start 2026-02-20T03:13 --end 2026-02-28T00:00 \
  --balance 200 \
  --sweep LATENCY_ARB_MOMENTUM_THRESHOLD=0.0010:0.0030:0.0002 \
  --sweep LATENCY_ARB_MAX_ASK=0.55,0.60,0.65 \
  --sweep MAX_POSITION_FRACTION=0.05,0.075,0.10,0.125 \
  --set SPREAD_CAPTURE_THRESHOLD=0.50
```

- `--param NAME=start:end:step` generates a range; `--param NAME=a,b,c` enumerates values
- `--set NAME=value` fixes a parameter without sweeping it
- Ticks are loaded once and cached across all combinations
- Results CSV has one row per combination with PnL, win rate, trades, max drawdown

The 275-combination sweep from run 006 data completed in ~40 minutes.

### Sweep Results

| Sweep | Status | Best PnL | Notes |
|---|---|---|---|
| 001 | INVALID | $14,602 | Contaminated by stale temp DBs — inflated starting balance |
| 002 | Valid (throttled) | $1,282 | v0.5 peak DD pause triggers at $60 loss from $200 start |
| 003 | **Baseline** | $4,070 | DD pause disabled — matches live results, mom=0.0012 dominates |

**Peak DD pause vs backtesting:** The v0.5 `PEAK_DD_PAUSE_PCT=0.30` fires after
~$60 loss on a $200 balance, pausing for 1 hour and missing windows. Live run 006
(v0.4, no DD pause) made $4,765. Disabling the pause in backtests
(`--set PEAK_DD_PAUSE_PCT=1.0`) reproduces live-like results (PnL=$3,314,
peak=$4,780). Always disable for parameter sweeps.

### Walk-Forward Validation

To avoid overfitting, split data into train/test periods:

1. **Train**: sweep on earlier data (e.g., Feb 20 -- Feb 28)
2. **Test**: run top candidates on later data (e.g., Feb 28 -- Mar 4)
3. Parameters good on both = robust; good on train only = overfit

### Key Limitations

- **Spread-capture overcounts ~18x**: 1-second tick sampling captures transient
  sub-threshold CLOB states that the event-driven live bot doesn't trigger on.
  Disable for sweeps with `--set SPREAD_CAPTURE_THRESHOLD=0.50`.
- **Latency-arb reproduction is excellent**: 257 vs 260 trades, 149 vs 149 wins
  (56.0% vs 57.3% WR) against run 006 live data.

## Data Collection Guide

### Short validation run (5-15 minutes)

```bash
rm -f data/buba-paint.db
npm start
# Wait for 2-3 window rollovers, then Ctrl+C
sqlite3 data/buba-paint.db "SELECT source, COUNT(*) FROM tick_data GROUP BY source;"
```

### Long AWS run

```bash
mkdir -p runs/NNN
DB_PATH=runs/NNN/buba-paint.db nohup timeout 18000 npx tsx src/main.ts \
  > runs/NNN/bot.log 2>&1 &

# Monitor
tail -f runs/NNN/bot.log
```

After collection, rsync `runs/NNN/` to your analysis machine and run the
Python scripts.

### Checking data health while running

```bash
# Row counts per table
sqlite3 data/buba-paint.db "
  SELECT 'tick_data' t, COUNT(*) FROM tick_data
  UNION SELECT 'markets',  COUNT(*) FROM markets
  UNION SELECT 'signals',  COUNT(*) FROM signals
  UNION SELECT 'trades',   COUNT(*) FROM simulated_trades
  UNION SELECT 'results',  COUNT(*) FROM trade_results
  UNION SELECT 'balance',  COUNT(*) FROM balance_log;
"

# Latest prices from each feed
sqlite3 data/buba-paint.db "
  SELECT source,
    ROUND(MAX(price), 2) AS last_price,
    ROUND(MAX(bid), 3)   AS last_bid,
    ROUND(MAX(ask), 3)   AS last_ask,
    datetime(MAX(timestamp)/1000, 'unixepoch') AS last_seen
  FROM tick_data GROUP BY source;
"

# Recent signals
sqlite3 data/buba-paint.db "
  SELECT datetime(timestamp/1000, 'unixepoch') AS time, strategy, direction,
    ROUND(up_ask, 3), ROUND(down_ask, 3)
  FROM signals ORDER BY timestamp DESC LIMIT 10;
"

# Trade results summary
sqlite3 data/buba-paint.db "
  SELECT t.strategy, COUNT(*) trades,
    SUM(CASE WHEN r.pnl_0pct > 0 THEN 1 ELSE 0 END) wins,
    ROUND(SUM(r.pnl_0pct), 2) total_pnl_0,
    ROUND(SUM(r.pnl_3pct), 2) total_pnl_3
  FROM trade_results r
  JOIN simulated_trades t ON r.trade_id = t.id
  GROUP BY t.strategy;
"

# Bankroll history
sqlite3 data/buba-paint.db "
  SELECT datetime(timestamp/1000, 'unixepoch') AS time,
    event, trade_id, ROUND(amount, 2) AS amount, ROUND(balance, 2) AS balance
  FROM balance_log ORDER BY timestamp;
"
```

## Agent Guide

This section is for AI agents (like Claude Code) resuming work on this project.

### Restoring Context

1. Read this README for architecture, data model, and bankroll mechanics.
2. Read `src/config.ts` for all configurable parameters and their defaults.
3. Read `src/types.ts` for the TypeScript interface definitions used across all
   modules.
4. The database schema is in `src/db/schema.ts` -- 6 tables, all column names
   are snake_case.
5. The database wrapper is in `src/db/database.ts` -- note that
   `getOpenTradesForMarket` maps snake_case DB rows to camelCase TypeScript
   interfaces.
6. Read `src/bankroll-manager.ts` for position sizing logic, Kelly criterion,
   and safety limits.
7. Read `src/position-manager.ts` for trade lifecycle: opposing position guard,
   bankroll integration, and P&L resolution.
8. Run data is in `runs/NNN/` directories (each with `buba-paint.db` and
   `bot.log`).

### Running the Bot

```bash
# Quick smoke test (15 seconds)
timeout 15 npx tsx src/main.ts

# Debug mode to see all book updates and spread diagnostics
LOG_LEVEL=debug timeout 15 npx tsx src/main.ts

# Full run, backgrounded with timeout
mkdir -p runs/NNN
DB_PATH=runs/NNN/buba-paint.db nohup timeout 3600 npx tsx src/main.ts \
  > runs/NNN/bot.log 2>&1 &
```

### Generating Visualizations

```bash
python3 scripts/latency_distribution.py runs/NNN/buba-paint.db
python3 scripts/spread_over_time.py     runs/NNN/buba-paint.db
python3 scripts/pnl_curve.py            runs/NNN/buba-paint.db
python3 scripts/signal_frequency.py     runs/NNN/buba-paint.db
python3 scripts/binance_vs_chainlink.py runs/NNN/buba-paint.db
```

Output PNGs are written to the current working directory. To inspect them as a
vision-capable agent, read the PNG file directly with the Read tool.

### Ad-hoc Analysis Queries

```sql
-- Spread distribution: how often is UP+DOWN ask below $1?
SELECT
  ROUND(u.ask + d.ask, 2) AS total_ask,
  COUNT(*) AS samples
FROM tick_data u
JOIN tick_data d ON d.source = 'clob_down' AND d.timestamp = u.timestamp
WHERE u.source = 'clob_up' AND u.ask > 0 AND d.ask > 0
GROUP BY ROUND(u.ask + d.ask, 2)
ORDER BY total_ask;

-- Binance-Chainlink lag per second
SELECT
  b.timestamp / 1000 AS second,
  ROUND(b.price - c.price, 2) AS delta
FROM tick_data b
JOIN tick_data c ON c.source = 'chainlink' AND ABS(c.timestamp - b.timestamp) < 1500
WHERE b.source = 'binance'
ORDER BY b.timestamp;

-- Window-by-window summary
SELECT
  m.question,
  m.status,
  COUNT(t.id) AS trades,
  SUM(CASE WHEN r.pnl_0pct > 0 THEN 1 ELSE 0 END) AS wins,
  ROUND(SUM(r.pnl_0pct), 2) AS pnl
FROM markets m
LEFT JOIN simulated_trades t ON t.market_id = m.market_id
LEFT JOIN trade_results r ON r.trade_id = t.id
GROUP BY m.market_id
ORDER BY m.end_time DESC
LIMIT 20;

-- Bankroll curve
SELECT
  datetime(timestamp/1000, 'unixepoch') AS time,
  event, ROUND(balance, 2) AS balance
FROM balance_log ORDER BY timestamp;
```

### Strategy Interface

The strategy interface is in `src/types.ts`:

```typescript
interface Strategy {
  readonly name: string;
  evaluate(ctx: StrategyContext): Signal | Signal[] | null;
}

interface StrategyContext {
  binancePrice: number;
  binanceMomentum: number;        // (latest - oldest) / oldest over MOMENTUM_WINDOW_MS
  chainlinkPrice: number | null;
  bookState: BookState;           // .up and .down TopOfBook (bestBid, bestAsk, sizes)
  windowTimeRemainingMs: number;
}

interface Signal {
  timestamp: number;
  strategy: string;
  direction: "UP" | "DOWN";
  confidence: number;             // 0.0 to 1.0, scales position size
  binancePrice: number;
  chainlinkPrice: number;
  upAsk: number;
  downAsk: number;
  upBid: number;
  downBid: number;
  metadata: Record<string, unknown>;
}
```

To add a new strategy:

1. Create `src/strategies/my-strategy.ts` implementing `Strategy`.
2. Add it to the `strategies` array in `src/main.ts`.
3. Return a `Signal` (single), `Signal[]` (multiple, e.g. spread capture buys
   both sides), or `null` (no opportunity).
4. Signals are automatically logged to the `signals` table, filtered by the
   trend tracker, and fed to the position manager for bankroll-aware sizing.

### Key Implementation Details

- Market discovery uses slug `btc-updown-5m-{floor(unix_seconds/300)*300}`
  against `GET /events/slug/{slug}`. The generic `/markets` endpoint does NOT
  return these short-lived markets.

- CLOB initial book arrives as an array of objects with `asset_id`, `bids`,
  `asks` but NO `event_type` field. Subsequent updates have
  `event_type: "price_change"` with a `price_changes` array containing
  `best_bid` and `best_ask`.

- Chainlink RTDS sends an initial data dump as `{"payload":{"data":[...]}}`
  (no `topic` field) followed by live updates as
  `{"topic":"crypto_prices_chainlink","payload":{...}}`.

- Chainlink staleness detection: if no data arrives for `CHAINLINK_STALE_MS`
  (default 30s), the feed resets its tracking state and force-reconnects. On
  each successful reconnect the staleness timer resets, allowing repeated
  retries if the server continues to withhold data. During staleness
  `getPrice()` returns null, causing both tick logging and settlement to fall
  back to Binance.

- Binance momentum is computed as `(latest_price - oldest_price) / oldest_price`
  over a rolling window of recent aggTrade prices (default 30 seconds).

- P&L resolution: at window close, if Chainlink close >= Chainlink open, outcome
  is UP. Winning side settles at $1, losing at $0. Fee is deducted as a
  percentage of entry cost.

- Position sizing: `size` in `simulated_trades` is a token count (not USD).
  Actual USD cost = `entry_price * size`. The bankroll manager computes the
  token count based on balance fraction, Kelly criterion, and confidence.

- Opposing position guard: for single signals (latency-arb), blocks the same
  strategy from opening a second position in the same 5-minute window. For
  batch signals (spread-capture returns `Signal[]`), only blocks exact
  duplicates (same strategy + same direction), allowing both sides to open
  atomically. This prevents latency-arb from hedging against itself while
  letting spread-capture buy both sides as intended.

- Strategies are evaluated on both Binance tick and CLOB book update events,
  throttled to a minimum 200ms interval to avoid redundant evaluations.

- SQLite uses WAL mode so Python scripts can read while the bot writes.

- DB column names are snake_case, TypeScript interfaces are camelCase. The
  `database.ts` mapper in `getOpenTradesForMarket` handles the translation.

### Common Issues

| Symptom                             | Cause                            | Fix                                                   |
| ----------------------------------- | -------------------------------- | ----------------------------------------------------- |
| "No active 5-min BTC market found"  | Between windows or API hiccup    | Retries automatically. Check polymarket.com/crypto/5M  |
| Chainlink price unavailable at open | Chainlink WS connects after      | Falls back to Binance price; captures on first tick    |
|                                     | discovery fires                  | if both unavailable at window open                     |
| No signals generated                | Thresholds too tight for current | Lower LATENCY_ARB_MOMENTUM_THRESHOLD                   |
|                                     | market conditions                | or raise SPREAD_CAPTURE_THRESHOLD                      |
| CLOB bid/ask all null               | Book snapshot format changed     | Check raw messages with LOG_LEVEL=debug                |
|                                     |                                  | and update clob-feed.ts parsing                        |
| "Balance below minimum"             | Drawdown hit safety limit        | Increase STARTING_BALANCE or MIN_BALANCE_THRESHOLD     |
| Chainlink feed stale (flat line)    | RTDS stopped sending data while  | Auto-detected after CHAINLINK_STALE_MS (30s);          |
|                                     | WebSocket stayed connected       | force-reconnects until data resumes                    |
| Spread capture never fires          | CLOB too efficient at 5-min      | Expected behavior; may fire on future 1-min markets    |

### Run History

| Run | Duration  | Signals | Trades | Win Rate | P&L (0%) | P&L (3%) | Notes                        |
| --- | --------- | ------- | ------ | -------- | -------- | -------- | ---------------------------- |
| 002 | 5 hours   | 12      | 9      | 55.6%    | +$69.00  | +$56.07  | v0.1, fixed 100-token bets   |
| 003 | 1 hour    | 1       | 1      | 0%       | -$11.00  | -$11.33  | v0.2, bankroll-aware sizing  |
| 004 | 96 hours  | 161     | 76     | 51.3%    | +$719    | --       | v0.2, $200→$919, peak $1556  |
| 005 | 25 hours  | 37      | 11     | 36.4%    | -$4.86   | -$7.29   | v0.3, over-filtering bug     |
| 006 | 267 hours | 583     | 292    | 56.5%    | +$4,565  | +$3,467  | v0.4, $200→$4,765, peak $9,678, 50.8% max DD |
| 007 | ongoing   | --      | --     | --       | --       | --       | v0.5, $200 start, deployed Mar 10             |

## Out of Scope (for now)

- Real order execution, wallet integration, private keys
- Copy-trading / wallet tracking
- Maker order strategy (place limit orders to avoid taker fees)
- Rust/C++ latency optimizations
- Docker, CI/CD, deployment automation
- Any UI beyond console logs and Python plots
