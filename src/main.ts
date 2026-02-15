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
    `momentum_window=${CONFIG.MOMENTUM_WINDOW_MS}ms, ` +
    `spread_threshold=${CONFIG.SPREAD_CAPTURE_THRESHOLD}, ` +
    `cooldown=${CONFIG.LATENCY_ARB_COOLDOWN_MS}ms, ` +
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
  let currentWindowId: string | null = null;

  // === Strategy Evaluation (shared by Binance + CLOB triggers) ===

  let lastEvalTime = 0;
  const EVAL_INTERVAL_MS = 200;

  function runStrategies(): void {
    const now = Date.now();
    if (now - lastEvalTime < EVAL_INTERVAL_MS) return;
    lastEvalTime = now;

    const window = discovery.getCurrentWindow();
    if (!window) return;

    const binPrice = binanceFeed.getPrice();
    if (binPrice === null) return;

    // Fix startup race: if windowOpenPrice is still null and we have a
    // current window, capture it now from the first available price.
    if (windowOpenPrice === null && currentWindowId === window.marketId) {
      windowOpenPrice = chainlinkFeed.getPrice() ?? binanceFeed.getPrice();
      if (windowOpenPrice !== null) {
        log.info(`Window open price (late capture): $${windowOpenPrice.toFixed(2)}`);
      }
    }

    const ctx: StrategyContext = {
      binancePrice: binPrice,
      binanceMomentum: binanceFeed.getMomentum(),
      chainlinkPrice: chainlinkFeed.getPrice(),
      bookState: clobFeed.getBookState(),
      windowTimeRemainingMs: window.endTime - now,
    };

    for (const strategy of strategies) {
      const result = strategy.evaluate(ctx);
      if (result === null) continue;

      const signals = Array.isArray(result) ? result : [result];
      for (const signal of signals) {
        log.info(`SIGNAL: ${signal.strategy} => ${signal.direction} | ` +
          `momentum=${(ctx.binanceMomentum * 100).toFixed(4)}% | ` +
          `UP ask=${signal.upAsk.toFixed(3)} DOWN ask=${signal.downAsk.toFixed(3)}`);
        db.logSignal(signal);
        positionManager.tryOpen(signal, window);
      }
    }
  }

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
    currentWindowId = window.marketId;
    windowOpenPrice = chainlinkFeed.getPrice();
    if (windowOpenPrice !== null) {
      log.info(`Window open price (Chainlink): $${windowOpenPrice.toFixed(2)}`);
    } else {
      windowOpenPrice = binanceFeed.getPrice();
      if (windowOpenPrice !== null) {
        log.warn(`Chainlink unavailable at window open — Binance fallback: $${windowOpenPrice.toFixed(2)}`);
      } else {
        log.warn("No price available at window open — will capture on first tick");
      }
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
      log.warn("Cannot resolve window: missing price data — marking as closed");
      db.resolveMarket(window.marketId, "closed");
    }

    windowOpenPrice = null;
    currentWindowId = null;
  });

  // Strategy evaluation triggers
  binanceFeed.on("tick", runStrategies);
  clobFeed.on("book", runStrategies);
  clobFeed.on("priceChange", runStrategies);

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
