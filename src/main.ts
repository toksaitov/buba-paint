import { CONFIG } from "./config.js";
import { Database } from "./db/database.js";
import { BinanceFeed } from "./feeds/binance-feed.js";
import { ClobFeed } from "./feeds/clob-feed.js";
import { ChainlinkFeed } from "./feeds/chainlink-feed.js";
import { MarketDiscovery } from "./market-discovery.js";
import { LatencyArbStrategy } from "./strategies/latency-arb.js";
import { SpreadCaptureStrategy } from "./strategies/spread-capture.js";
import { PositionManager } from "./position-manager.js";
import { TickLogger } from "./tick-logger.js";
import { createLogger } from "./utils/logger.js";
import type { StrategyContext, MarketWindow, Strategy } from "./types.js";

const log = createLogger("main");

async function main(): Promise<void> {
  log.info("=== buba-paint paper trading bot ===");
  log.info(`Config: momentum_threshold=${CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD}, ` +
    `spread_threshold=${CONFIG.SPREAD_CAPTURE_THRESHOLD}, ` +
    `position_size=$${CONFIG.POSITION_SIZE}`);

  // 1. Database
  const db = new Database(CONFIG.DB_PATH);

  // 2. Feeds
  const binanceFeed = new BinanceFeed();
  const clobFeed = new ClobFeed();
  const chainlinkFeed = new ChainlinkFeed();

  // 3. Market discovery
  const discovery = new MarketDiscovery();

  // 4. Strategies
  const strategies: Strategy[] = [
    new LatencyArbStrategy(),
    new SpreadCaptureStrategy(),
  ];

  // 5. Position manager
  const positionManager = new PositionManager(db);

  // 6. Tick logger
  const tickLogger = new TickLogger(db, binanceFeed, clobFeed, chainlinkFeed);

  // Track Chainlink price at window open for settlement
  let windowOpenPrice: number | null = null;

  // === Event Wiring ===

  // Feed status logging
  binanceFeed.on("connected", () => log.info("Binance feed connected"));
  binanceFeed.on("disconnected", () => log.warn("Binance feed disconnected"));
  chainlinkFeed.on("connected", () => log.info("Chainlink feed connected"));
  chainlinkFeed.on("disconnected", () => log.warn("Chainlink feed disconnected"));
  clobFeed.on("connected", () => log.info("CLOB feed connected"));
  clobFeed.on("disconnected", () => log.warn("CLOB feed disconnected"));

  // New 5-min window discovered
  discovery.on("newWindow", (window: MarketWindow) => {
    db.upsertMarket(window);
    clobFeed.resubscribe(window.upTokenId, window.downTokenId);
    windowOpenPrice = chainlinkFeed.getPrice();
    if (windowOpenPrice !== null) {
      log.info(`Window open price (Chainlink): $${windowOpenPrice.toFixed(2)}`);
    } else {
      log.warn("Chainlink price unavailable at window open — using Binance fallback");
      windowOpenPrice = binanceFeed.getPrice();
    }
  });

  // Window closed — resolve positions
  discovery.on("windowClosed", (window: MarketWindow) => {
    let closePrice = chainlinkFeed.getPrice();
    if (closePrice === null) {
      log.warn("Chainlink price unavailable at window close — using Binance fallback");
      closePrice = binanceFeed.getPrice();
    }

    if (windowOpenPrice !== null && closePrice !== null) {
      positionManager.resolveWindow(window, windowOpenPrice, closePrice);
    } else {
      log.error("Cannot resolve window: missing price data");
    }

    windowOpenPrice = null;
  });

  // Strategy evaluation — throttled to avoid excessive evaluation on high-frequency ticks
  let lastEvalTime = 0;
  const EVAL_INTERVAL_MS = 200; // Evaluate at most every 200ms

  binanceFeed.on("tick", () => {
    const now = Date.now();
    if (now - lastEvalTime < EVAL_INTERVAL_MS) return;
    lastEvalTime = now;

    const window = discovery.getCurrentWindow();
    if (!window) return;

    const binPrice = binanceFeed.getPrice();
    if (binPrice === null) return;

    const ctx: StrategyContext = {
      binancePrice: binPrice,
      binanceMomentum: binanceFeed.getMomentum(),
      chainlinkPrice: chainlinkFeed.getPrice(),
      bookState: clobFeed.getBookState(),
      windowTimeRemainingMs: window.endTime - now,
    };

    for (const strategy of strategies) {
      const signal = strategy.evaluate(ctx);
      if (signal) {
        log.info(`SIGNAL: ${signal.strategy} => ${signal.direction} | ` +
          `momentum=${(ctx.binanceMomentum * 100).toFixed(4)}% | ` +
          `UP ask=${signal.upAsk.toFixed(3)} DOWN ask=${signal.downAsk.toFixed(3)}`);
        db.logSignal(signal);
        positionManager.tryOpen(signal, window);
      }
    }
  });

  // === Start Everything ===
  log.info("Connecting feeds...");
  binanceFeed.connect();
  chainlinkFeed.connect();

  log.info("Starting market discovery...");
  await discovery.start();

  tickLogger.start();

  log.info("All systems running. Press Ctrl+C to stop.");

  // === Graceful Shutdown ===
  let shuttingDown = false;
  const shutdown = () => {
    if (shuttingDown) return;
    shuttingDown = true;

    log.info("Shutting down...");
    tickLogger.stop();
    discovery.stop();
    binanceFeed.disconnect();
    clobFeed.disconnect();
    chainlinkFeed.disconnect();
    db.close();
    log.info("Shutdown complete.");
    process.exit(0);
  };

  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
