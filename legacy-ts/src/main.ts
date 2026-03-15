import { CONFIG } from "./config.js";
import { Database } from "./db/database.js";
import { BinanceFeed } from "./feeds/binance-feed.js";
import { ClobFeed } from "./feeds/clob-feed.js";
import { ChainlinkFeed } from "./feeds/chainlink-feed.js";
import { MarketDiscovery } from "./market-discovery.js";
import { LatencyArbStrategy } from "./strategies/latency-arb.js";
import { SpreadCaptureStrategy } from "./strategies/spread-capture.js";
import { PositionManager } from "./position-manager.js";
import { BankrollManager } from "./bankroll-manager.js";
import { CircuitBreaker } from "./circuit-breaker.js";
import { RegimeDetector } from "./regime-detector.js";
import { TrendTracker } from "./trend-tracker.js";
import { TickLogger } from "./tick-logger.js";
import { createLogger } from "./utils/logger.js";
import type { StrategyContext, MarketWindow, Strategy, SignalDirection } from "./types.js";

const log = createLogger("main");

async function main(): Promise<void> {
  log.info("=== buba-paint paper trading bot v0.5 ===");
  log.info(`Config: momentum=${CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD}, ` +
    `window=${CONFIG.MOMENTUM_WINDOW_MS}ms, ` +
    `cooldown=${CONFIG.LATENCY_ARB_COOLDOWN_MS}ms, ` +
    `min_ask=${CONFIG.LATENCY_ARB_MIN_ASK}, ` +
    `balance=$${CONFIG.STARTING_BALANCE}, ` +
    `max_fraction=${(CONFIG.MAX_POSITION_FRACTION * 100).toFixed(0)}%, ` +
    `max_pos_usd=${(CONFIG.MAX_POSITION_USD_FRACTION * 100).toFixed(0)}%, ` +
    `kelly=${CONFIG.KELLY_FRACTION}, ` +
    `circuit_breaker=${CONFIG.CIRCUIT_BREAKER_LOSSES}L/${(CONFIG.CIRCUIT_BREAKER_PAUSE_MS / 60_000).toFixed(0)}min`);

  // 1. Database
  const db = new Database(CONFIG.DB_PATH);

  // 2. Bankroll manager
  const bankroll = new BankrollManager(CONFIG.STARTING_BALANCE, db);

  // 3. Feeds
  const binanceFeed = new BinanceFeed();
  const clobFeed = new ClobFeed();
  const chainlinkFeed = new ChainlinkFeed();

  // 4. Market discovery
  const discovery = new MarketDiscovery();

  // 5. Strategies
  const strategies: Strategy[] = [
    new LatencyArbStrategy(),
    new SpreadCaptureStrategy(),
  ];

  // 6. Position manager (with bankroll)
  const positionManager = new PositionManager(db, bankroll);

  // 7. Trend tracker
  const trendTracker = new TrendTracker();

  // 8. Tick logger
  const tickLogger = new TickLogger(db, binanceFeed, clobFeed, chainlinkFeed);

  // 9. Circuit breaker — pauses trading after consecutive losses
  const circuitBreaker = new CircuitBreaker();

  // 10. Regime detector (experimental, off by default)
  const regimeDetector = CONFIG.REGIME_DETECTION_ENABLED ? new RegimeDetector() : null;
  if (regimeDetector) log.info("Regime detection ENABLED");

  // Track Chainlink price at window open for settlement
  let windowOpenPrice: number | null = null;
  let currentWindowId: string | null = null;

  // Periodic spread diagnostic (every 60s, debug level)
  let lastSpreadLogTime = 0;

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

    // Periodic spread diagnostic
    if (now - lastSpreadLogTime > 60_000 && ctx.bookState.up && ctx.bookState.down) {
      const totalAsk = ctx.bookState.up.bestAsk + ctx.bookState.down.bestAsk;
      log.debug(`Spread check: UP ask=${ctx.bookState.up.bestAsk.toFixed(3)} + ` +
        `DOWN ask=${ctx.bookState.down.bestAsk.toFixed(3)} = ${totalAsk.toFixed(4)} ` +
        `(threshold: ${CONFIG.SPREAD_CAPTURE_THRESHOLD})`);
      lastSpreadLogTime = now;
    }

    // Circuit breaker: skip signal processing if paused
    if (!circuitBreaker.canTrade()) return;

    // Feed regime detector (experimental)
    if (regimeDetector) {
      regimeDetector.addPrice(binPrice, now);
    }

    for (const strategy of strategies) {
      const result = strategy.evaluate(ctx);
      if (result === null) continue;

      // Regime filter: suppress latency-arb in choppy markets
      if (regimeDetector && strategy.name === "latency-arb") {
        const regime = regimeDetector.getRegime();
        if (regime === "choppy") {
          const signals = Array.isArray(result) ? result : [result];
          for (const signal of signals) {
            log.info(`SIGNAL SUPPRESSED (choppy regime): ${signal.strategy} => ${signal.direction}`);
            db.logSignal(signal);
          }
          continue;
        }
      }

      const signals = Array.isArray(result) ? result : [result];
      const isBatch = Array.isArray(result) && result.length > 1;

      if (isBatch) {
        // Batch signals (spread-capture): balanced sizing, skip trend filter
        for (const signal of signals) {
          log.info(`SIGNAL: ${signal.strategy} => ${signal.direction} | ` +
            `confidence=${signal.confidence.toFixed(2)} | ` +
            `UP ask=${signal.upAsk.toFixed(3)} DOWN ask=${signal.downAsk.toFixed(3)} | ` +
            `totalAsk=${(signal.upAsk + signal.downAsk).toFixed(4)}`);
          db.logSignal(signal);
        }
        positionManager.tryOpenSpread(signals, window);
      } else {
        for (const signal of signals) {
          // Trend filter (experimental, off by default)
          if (trendTracker.shouldSuppress(signal.direction)) {
            log.info(`SIGNAL SUPPRESSED (trend): ${signal.strategy} => ${signal.direction}`);
            db.logSignal(signal);
            continue;
          }

          log.info(`SIGNAL: ${signal.strategy} => ${signal.direction} | ` +
            `confidence=${signal.confidence.toFixed(2)} | ` +
            `momentum=${(ctx.binanceMomentum * 100).toFixed(4)}% | ` +
            `UP ask=${signal.upAsk.toFixed(3)} DOWN ask=${signal.downAsk.toFixed(3)}`);
          db.logSignal(signal);
          positionManager.tryOpen(signal, window);
        }
      }
    }
  }

  // === Event Wiring ===

  // Feed status logging
  binanceFeed.on("connected", () => log.info("Binance feed connected"));
  binanceFeed.on("disconnected", () => log.warn("Binance feed disconnected"));
  chainlinkFeed.on("connected", () => log.info("Chainlink feed connected"));
  chainlinkFeed.on("disconnected", () => log.warn("Chainlink feed disconnected"));
  chainlinkFeed.on("stale", () => log.warn("Chainlink feed stale — prices will fall back to Binance until reconnect"));
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

  // Window closed — resolve positions and log balance
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

    // Log bankroll status after each window
    const stats = bankroll.getStats();
    log.info(
      `BANKROLL: $${stats.currentBalance.toFixed(2)} | ` +
      `P&L=$${stats.totalPnl.toFixed(2)} | ` +
      `W/L=${stats.wins}/${stats.losses} (${(stats.winRate * 100).toFixed(0)}%) | ` +
      `drawdown=${(stats.maxDrawdownPct * 100).toFixed(1)}%`,
    );

    windowOpenPrice = null;
    currentWindowId = null;
  });

  // Wire trade resolution to trend tracker + circuit breaker via a wrapper
  const origResolve = positionManager.resolveWindow.bind(positionManager);
  positionManager.resolveWindow = (window: MarketWindow, openPrice: number, closePrice: number) => {
    const outcome: SignalDirection = closePrice >= openPrice ? "UP" : "DOWN";
    const trades = db.getOpenTradesForMarket(window.marketId);
    origResolve(window, openPrice, closePrice);
    // Record outcomes for trend tracker + circuit breaker
    for (const trade of trades) {
      const won = trade.side === outcome;
      trendTracker.recordOutcome(trade.side, won);
      circuitBreaker.recordResult(won);
    }
  };

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
    const stats = bankroll.getStats();
    log.info(
      `FINAL BANKROLL: $${stats.currentBalance.toFixed(2)} | ` +
      `P&L=$${stats.totalPnl.toFixed(2)} | ` +
      `W/L=${stats.wins}/${stats.losses} | ` +
      `max drawdown=${(stats.maxDrawdownPct * 100).toFixed(1)}%`,
    );
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
