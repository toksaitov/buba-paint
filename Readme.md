# buba-paint

Paper-trading system for Polymarket 5-minute BTC Up/Down prediction markets.
Connects to three live WebSocket feeds, detects latency-arbitrage and
spread-capture opportunities, simulates trades, logs everything to SQLite, and
produces analysis visualizations.

No real orders, no wallet, no private keys. This is a data-collection and
strategy-validation tool.

## How It Works

Every 5 minutes, Polymarket opens a new market asking "Will BTC go Up or Down
in the next 5 minutes?" resolved by a Chainlink oracle. The bot exploits two
potential edges:

Strategy A -- Latency Arb. Binance BTC spot price moves faster than the
Polymarket CLOB order book reprices. When Binance shows strong momentum
(configurable threshold, default 0.3% over 10 seconds) but the CLOB still shows
near-50/50 odds, the bot logs a simulated directional buy at the current best
ask.

Strategy B -- Spread Capture. When the sum of the best ask for the UP token and
the best ask for the DOWN token falls below $1.00 (configurable threshold,
default $0.98), the market is theoretically mispriced. The bot logs a simulated
buy of the cheaper side.

At each 5-minute window close, open positions are resolved using the Chainlink
settlement price, and P&L is computed under four fee assumptions (0%, 1%, 2%,
3%).

## Architecture

The system is a single Node.js process with these components:

- `main.ts` -- orchestrator that wires everything together, handles startup and
  graceful shutdown.
- Three WebSocket feeds run concurrently, each with auto-reconnect:
    - `binance-feed.ts` reads Binance aggTrade, computes rolling momentum.
    - `clob-feed.ts` reads Polymarket CLOB, maintains top-of-book for UP/DOWN.
    - `chainlink-feed.ts` reads Polymarket RTDS, tracks Chainlink oracle price.
- `market-discovery.ts` polls the Gamma API every 60s to find the active
  5-minute BTC window, emits `newWindow` / `windowClosed` events.
- On `newWindow`, `clob-feed` resubscribes with the new token IDs.
- On each Binance tick, two strategies evaluate the current state:
    - `latency-arb.ts` -- fires when momentum is high but CLOB odds are stale.
    - `spread-capture.ts` -- fires when UP ask + DOWN ask < threshold.
- `position-manager.ts` opens simulated trades from signals, resolves them at
  window close using the Chainlink price, computes P&L at 4 fee levels.
- `tick-logger.ts` samples all feeds every 1s into the `tick_data` table.
- All data is persisted to a SQLite database (WAL mode).

Data flow: Binance/CLOB/Chainlink feeds --> strategies evaluate --> signals
logged --> positions opened --> window closes --> positions resolved --> P&L
recorded. Tick logger runs independently on a 1-second timer.

### Three Live Feeds

| Feed                      | URL                                                    | Data                                                  | Rate             |
|                           |                                                        |                                                       |                  |
| Binance aggTrade          | `wss://stream.binance.com:9443/ws/btcusdt@aggTrade`    | Per-trade BTC/USDT price and quantity                 | ~20-100 msgs/sec |
| Polymarket CLOB           | `wss://ws-subscriptions-clob.polymarket.com/ws/market` | Order book snapshots and price changes for UP/DOWN    | Variable         |
| Polymarket RTDS Chainlink | `wss://ws-live-data.polymarket.com`                    | Chainlink BTC/USD oracle price (settlement reference) | ~1 msg/sec       |

All feeds auto-reconnect with exponential backoff (1s base, 30s max, with
jitter).

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

## Project Structure

```
buba-paint/
  package.json             # ESM project, tsx scripts
  tsconfig.json            # Strict, ES2022, bundler resolution
  .env.example             # All configurable thresholds with docs
  src/
    main.ts                # Orchestrator: startup, event wiring, shutdown
    config.ts              # All constants from env vars with defaults
    types.ts               # Shared TypeScript interfaces
    db/
      schema.ts            # 5 SQLite tables with indexes
      database.ts          # better-sqlite3 wrapper, prepared statements
    feeds/
      base-feed.ts         # Abstract WebSocket base: connect, reconnect, ping
      binance-feed.ts      # aggTrade parser, rolling momentum window
      clob-feed.ts         # Book snapshots + price_change, top-of-book state
      chainlink-feed.ts    # RTDS subscribe, initial dump + live updates
    market-discovery.ts    # Gamma API event lookup by slug, window lifecycle
    strategies/
      latency-arb.ts       # Binance momentum vs stale CLOB odds
      spread-capture.ts    # UP ask + DOWN ask < threshold
    position-manager.ts    # Simulated trade open/resolve, multi-fee P&L
    tick-logger.ts         # 1s interval sampling all feeds to SQLite
    utils/
      logger.ts            # Timestamped structured logger with level filter
  scripts/
    latency_distribution.py
    spread_over_time.py
    pnl_curve.py
    signal_frequency.py
    binance_vs_chainlink.py
  data/                    # .gitignored; SQLite DB created at runtime
```

## Setup

```bash
# Install Node dependencies
npm install

# Copy and optionally edit config
cp .env.example .env

# Verify TypeScript compiles
npm run typecheck
```

Requires Node.js >= 22 (for native `fetch`). Python 3 with `matplotlib` and
`pandas` for analysis scripts.

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
LOG_LEVEL=debug LATENCY_ARB_MOMENTUM_THRESHOLD=0.005 npm start
```

### Long-running with pm2

```bash
npx pm2 start "npx tsx src/main.ts" --name buba-paint
npx pm2 logs buba-paint
npx pm2 stop buba-paint
```

The bot runs indefinitely, rolling through 5-minute windows. Press Ctrl+C for
graceful shutdown (all feeds disconnect, DB closes cleanly).

### What to Expect on Startup

```
[INFO] [main]      === buba-paint paper trading bot ===
[INFO] [db]        Database initialized at ./data/buba-paint.db
[INFO] [chainlink] Connected
[INFO] [chainlink] Initial data dump: 59 entries, latest $69710.35
[INFO] [discovery] New market window: Bitcoin Up or Down - February 14, 7:55PM-8:00PM ET
[INFO] [clob]      Subscribing to market {...token IDs...}
[INFO] [binance]   Connected
[INFO] [discovery] Window close scheduled in 252s
[INFO] [main]      All systems running.
```

Every 5 minutes you will see window close/open transitions. When strategies
fire:

```
[INFO] [main]      SIGNAL: latency-arb => UP | momentum=0.3521% | UP ask=0.510 DOWN ask=0.490
[INFO] [positions] TRADE OPENED #1: latency-arb UP @ 0.510 ($100 notional)
...
[INFO] [positions] TRADE RESOLVED #1: WIN latency-arb UP | entry=0.510 settlement=1 | P&L(0%)=$49.00 P&L(3%)=$47.47
```

## Database Schema

SQLite database at `data/buba-paint.db` (WAL mode, safe for concurrent reads
from Python).

### tick_data

Sampled every `TICK_INTERVAL` ms (default 1 second) from all feeds.

| Column    | Type    | Description                               |
|           |         |                                           |
| timestamp | INTEGER | Unix ms                                   |
| source    | TEXT    | binance, clob_up, clob_down, or chainlink |
| price     | REAL    | Spot price (binance, chainlink rows only) |
| bid       | REAL    | Best bid (clob_up, clob_down rows only)   |
| ask       | REAL    | Best ask (clob_up, clob_down rows only)   |
| bid_size  | REAL    | Size at best bid                          |
| ask_size  | REAL    | Size at best ask                          |

### markets

One row per 5-minute window the bot observed.

| Column        | Type    | Description                                   |
|               |         |                                               |
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

| Column          | Type    | Description                           |
|                 |         |                                       |
| timestamp       | INTEGER | Unix ms                               |
| strategy        | TEXT    | latency-arb or spread-capture         |
| direction       | TEXT    | UP or DOWN                            |
| binance_price   | REAL    | Binance BTC price at signal time      |
| chainlink_price | REAL    | Chainlink oracle price at signal time |
| up_ask          | REAL    | CLOB UP token best ask                |
| down_ask        | REAL    | CLOB DOWN token best ask              |
| up_bid          | REAL    | CLOB UP token best bid                |
| down_bid        | REAL    | CLOB DOWN token best bid              |
| metadata        | TEXT    | JSON with strategy-specific details   |

### simulated_trades

Opened when a signal passes position manager guards (max positions, no
duplicate).

| Column      | Type    | Description                   |
|             |         |                               |
| timestamp   | INTEGER | Unix ms                       |
| market_id   | TEXT    | FK to markets                 |
| strategy    | TEXT    | Which strategy opened this    |
| side        | TEXT    | UP or DOWN                    |
| token_id    | TEXT    | Which CLOB token was "bought" |
| entry_price | REAL    | Best ask at entry             |
| size        | REAL    | Notional USD (default $100)   |
| status      | TEXT    | open, closed, or expired      |

### trade_results

One row per resolved trade, joined to simulated_trades on trade_id.

| Column           | Type    | Description                           |
|                  |         |                                       |
| trade_id         | INTEGER | FK to simulated_trades                |
| exit_price       | REAL    | 1.0 (win) or 0.0 (loss)               |
| settlement_price | REAL    | Same as exit_price for binary markets |
| pnl_0pct         | REAL    | (settlement - entry) * size           |
| pnl_1pct         | REAL    | pnl_0pct minus 1% of entry cost       |
| pnl_2pct         | REAL    | pnl_0pct minus 2% of entry cost       |
| pnl_3pct         | REAL    | pnl_0pct minus 3% of entry cost       |
| resolved_at      | INTEGER | Unix ms when resolved                 |

## Configuration Reference

All settings via environment variables. Defaults shown.

| Variable                       | Default              | Description                                 |
|                                |                      |                                             |
| DB_PATH                        | ./data/buba-paint.db | SQLite database file path                   |
| LOG_LEVEL                      | info                 | debug, info, warn, or error                 |
| TICK_INTERVAL                  | 1000                 | Tick sampling interval in ms                |
| GAMMA_POLL_INTERVAL            | 60000                | How often to re-check Gamma API in ms       |
| LATENCY_ARB_MOMENTUM_THRESHOLD | 0.003                | Min Binance momentum fraction. 0.003 = 0.3% |
| LATENCY_ARB_MAX_ASK            | 0.55                 | Max CLOB ask to consider "stale" odds       |
| SPREAD_CAPTURE_THRESHOLD       | 0.98                 | Max UP+DOWN ask sum to fire spread capture  |
| MOMENTUM_WINDOW_MS             | 10000                | Rolling window for momentum calc in ms      |
| POSITION_SIZE                  | 100                  | Simulated position size in notional USD     |
| MAX_OPEN_POSITIONS             | 5                    | Max concurrent simulated positions          |
| MIN_WINDOW_TIME_MS             | 30000                | Don't enter trades with less than this left |

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
histogram and CDF of the delay in milliseconds.

Key metrics printed: mean, median, P95, P99 delay.
Output file: `latency_distribution.png`.

### spread_over_time.py

Time series of `UP_ask + DOWN_ask` across all sampled ticks. Shows how often the
combined ask dips below $1.00 (free money before fees) and the configured
threshold. Also plots individual UP and DOWN asks.

Key metrics printed: percentage of samples below $1.00 and below $0.98.
Output file: `spread_over_time.png`.

### pnl_curve.py

Cumulative P&L over time from resolved simulated trades. Top chart: all
strategies combined with 4 fee-level lines. Bottom chart: per-strategy breakdown
at 0% fee.

Key metrics printed: total trades, win rate, total and average P&L per strategy.
Output file: `pnl_curve.png`.

### signal_frequency.py

How many opportunities per hour each strategy detects, broken down by hour of
day (Eastern Time). Bar chart and cumulative signal count over time.

Key metrics printed: total signals per strategy, signals/hour rate.
Output file: `signal_frequency.png`.

### binance_vs_chainlink.py

Price delta between Binance spot and Chainlink oracle, bucketed to 1-second
intervals. Shows the price comparison, the absolute delta time series, and a
histogram of deltas. Market windows are shaded on the price chart.

Key metrics printed: mean, std, max, min delta.
Output file: `binance_vs_chainlink.png`.

## Data Collection Guide

### Short validation run (5-15 minutes)

```bash
rm -f data/buba-paint.db
npm start
# Wait for 2-3 window rollovers, then Ctrl+C
sqlite3 data/buba-paint.db "SELECT source, COUNT(*) FROM tick_data GROUP BY source;"
```

### Overnight data collection

```bash
rm -f data/buba-paint.db
nohup npx tsx src/main.ts > bot.log 2>&1 &
# Or with pm2:
npx pm2 start "npx tsx src/main.ts" --name buba-paint --log bot.log
```

After collection, copy `data/buba-paint.db` to your analysis machine and run the
Python scripts.

### Checking data health while running

```bash
# Row counts per table
sqlite3 data/buba-paint.db "
  SELECT 'tick_data' t, COUNT(*) FROM tick_data
  UNION SELECT 'markets', COUNT(*) FROM markets
  UNION SELECT 'signals', COUNT(*) FROM signals
  UNION SELECT 'trades', COUNT(*) FROM simulated_trades
  UNION SELECT 'results', COUNT(*) FROM trade_results;
"

# Latest prices from each feed
sqlite3 data/buba-paint.db "
  SELECT source,
    ROUND(MAX(price), 2) AS last_price,
    ROUND(MAX(bid), 3) AS last_bid,
    ROUND(MAX(ask), 3) AS last_ask,
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
```

## Agent Guide

This section is for AI agents (like Claude Code) resuming work on this project.

### Restoring Context

1. Read this README for architecture and data model.
2. Read `src/config.ts` for all configurable parameters and their defaults.
3. Read `src/types.ts` for the TypeScript interface definitions used across all
   modules.
4. The database schema is in `src/db/schema.ts` -- 5 tables, all column names
   are snake_case.
5. The database wrapper is in `src/db/database.ts` -- note that
   `getOpenTradesForMarket` maps snake_case DB rows to camelCase TypeScript
   interfaces.

### Running the Bot

```bash
# Quick smoke test (15 seconds)
timeout 15 npx tsx src/main.ts

# Debug mode to see all book updates
LOG_LEVEL=debug timeout 15 npx tsx src/main.ts

# Full run, backgrounded
nohup npx tsx src/main.ts > bot.log 2>&1 &
echo $! > bot.pid
```

### Generating Visualizations

```bash
python3 scripts/latency_distribution.py data/buba-paint.db
python3 scripts/spread_over_time.py data/buba-paint.db
python3 scripts/pnl_curve.py data/buba-paint.db
python3 scripts/signal_frequency.py data/buba-paint.db
python3 scripts/binance_vs_chainlink.py data/buba-paint.db
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
JOIN tick_data d ON d.source = 'clob_down' AND ABS(d.timestamp - u.timestamp) < 1500
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
```

### Iterating on Strategies

The strategy interface is in `src/types.ts`:

```typescript
interface Strategy {
  readonly name: string;
  evaluate(ctx: StrategyContext): Signal | null;
}

interface StrategyContext {
  binancePrice: number;
  binanceMomentum: number;        // (latest - oldest) / oldest over MOMENTUM_WINDOW_MS
  chainlinkPrice: number | null;
  bookState: BookState;           // .up and .down TopOfBook (bestBid, bestAsk, sizes)
  windowTimeRemainingMs: number;
}
```

To add a new strategy:

1. Create `src/strategies/my-strategy.ts` implementing `Strategy`.
2. Add it to the `strategies` array in `src/main.ts`.
3. Signals are automatically logged to the `signals` table and fed to the
   position manager.

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

- Binance momentum is computed as `(latest_price - oldest_price) / oldest_price`
  over a rolling window of recent aggTrade prices.

- P&L resolution: at window close, if Chainlink close >= Chainlink open, outcome
  is UP. Winning side settles at $1, losing at $0. Fee is deducted as a
  percentage of entry cost.

- SQLite uses WAL mode so Python scripts can read while the bot writes.

- DB column names are snake_case, TypeScript interfaces are camelCase. The
  `database.ts` mapper in `getOpenTradesForMarket` handles the translation.

### Common Issues

| Symptom                             | Cause                            | Fix                                                   |
|                                     |                                  |                                                       |
| "No active 5-min BTC market found"  | Between windows or API hiccup    | Retries automatically. Check polymarket.com/crypto/5M |
| Chainlink price unavailable at open | Chainlink WS connects after      | Falls back to Binance price automatically             |
|                                     | discovery fires                  |                                                       |
| No signals generated                | Thresholds too tight for current | Lower LATENCY_ARB_MOMENTUM_THRESHOLD                  |
|                                     | market conditions                | or raise SPREAD_CAPTURE_THRESHOLD                     |
| CLOB bid/ask all null               | Book snapshot format changed     | Check raw messages with LOG_LEVEL=debug               |
|                                     |                                  | and update clob-feed.ts parsing                       |

## Out of Scope (for now)

- Real order execution, wallet integration, private keys
- Copy-trading / wallet tracking
- Maker order strategy (place limit orders to avoid taker fees)
- Rust/C++ latency optimizations
- Docker, CI/CD, deployment automation
- Any UI beyond console logs and Python plots
