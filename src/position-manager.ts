import { EventEmitter } from "node:events";
import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { Database } from "./db/database.js";
import type { BankrollManager } from "./bankroll-manager.js";
import type { Signal, SimulatedTrade, TradeResult, MarketWindow } from "./types.js";

const log = createLogger("positions");

export class PositionManager extends EventEmitter {
  private openCount = 0;

  constructor(private db: Database, private bankroll: BankrollManager) {
    super();
  }

  tryOpen(signal: Signal, window: MarketWindow, isBatch = false): SimulatedTrade | null {
    // Guard: max open positions
    if (this.openCount >= CONFIG.MAX_OPEN_POSITIONS) {
      log.debug("Max open positions reached, skipping signal");
      return null;
    }

    // Guard: bankroll allows trading
    if (!this.bankroll.canTrade()) {
      return null;
    }

    // Guard: duplicate / opposing position prevention
    const existing = this.db.getOpenTradesForMarket(window.marketId);
    if (isBatch) {
      // Batch signals (e.g., spread-capture buys both sides atomically):
      // only block exact duplicates (same strategy + same direction)
      const duplicate = existing.find(
        (t) => t.strategy === signal.strategy && t.side === signal.direction,
      );
      if (duplicate) {
        log.debug(
          `Already have ${duplicate.strategy} ${duplicate.side} in ${window.marketId}, skipping duplicate`,
        );
        return null;
      }
    } else {
      // Single signals: block ANY position from the same strategy in this market
      // (prevents opposing UP+DOWN bets that guarantee a net loss)
      const sameStrategyTrade = existing.find(
        (t) => t.strategy === signal.strategy,
      );
      if (sameStrategyTrade) {
        log.debug(
          `Already have ${sameStrategyTrade.strategy} ${sameStrategyTrade.side} in ${window.marketId}, ` +
          `blocking ${signal.direction} (opposing position prevention)`,
        );
        return null;
      }
    }

    const entryPrice = signal.direction === "UP" ? signal.upAsk : signal.downAsk;
    const tokenId = signal.direction === "UP" ? window.upTokenId : window.downTokenId;

    // Bankroll-aware position sizing (per-strategy)
    const size = this.bankroll.reserveCapital(entryPrice, signal.confidence, signal.strategy);
    if (size <= 0) {
      log.debug(`Bankroll rejected trade: insufficient capital or no edge`);
      return null;
    }

    const trade: SimulatedTrade = {
      timestamp: signal.timestamp,
      marketId: window.marketId,
      strategy: signal.strategy,
      side: signal.direction,
      tokenId,
      entryPrice,
      size,
      status: "open",
    };

    const id = this.db.openTrade(trade);
    trade.id = id;
    this.openCount++;

    const cost = (entryPrice * size).toFixed(2);
    log.info(
      `TRADE OPENED #${id}: ${trade.strategy} ${trade.side} @ ${trade.entryPrice.toFixed(3)} ` +
      `(${size} tokens, $${cost} cost) [market: ${window.question}]`,
    );

    this.emit("tradeOpened", trade);
    return trade;
  }

  /**
   * Open a spread-capture pair (both UP and DOWN) with balanced sizing.
   */
  tryOpenSpread(signals: Signal[], window: MarketWindow): SimulatedTrade[] {
    // Guard: max open positions (need room for both legs)
    if (this.openCount + 2 > CONFIG.MAX_OPEN_POSITIONS) {
      log.debug("Not enough position slots for spread, skipping");
      return [];
    }

    if (!this.bankroll.canTrade()) return [];

    const upSignal = signals.find((s) => s.direction === "UP");
    const downSignal = signals.find((s) => s.direction === "DOWN");
    if (!upSignal || !downSignal) return [];

    // Guard: duplicate prevention
    const existing = this.db.getOpenTradesForMarket(window.marketId);
    for (const signal of [upSignal, downSignal]) {
      const duplicate = existing.find(
        (t) => t.strategy === signal.strategy && t.side === signal.direction,
      );
      if (duplicate) {
        log.debug(`Already have ${duplicate.strategy} ${duplicate.side}, skipping spread`);
        return [];
      }
    }

    // Balanced sizing: both legs sized together
    const { upTokens, downTokens } = this.bankroll.reserveSpreadCapital(
      upSignal.upAsk,
      downSignal.downAsk,
      upSignal.confidence,
    );
    if (upTokens <= 0 || downTokens <= 0) {
      log.debug("Bankroll rejected spread: insufficient capital");
      return [];
    }

    const trades: SimulatedTrade[] = [];
    const legs: Array<{ signal: Signal; size: number }> = [
      { signal: upSignal, size: upTokens },
      { signal: downSignal, size: downTokens },
    ];

    for (const { signal, size } of legs) {
      const entryPrice = signal.direction === "UP" ? signal.upAsk : signal.downAsk;
      const tokenId = signal.direction === "UP" ? window.upTokenId : window.downTokenId;

      const trade: SimulatedTrade = {
        timestamp: signal.timestamp,
        marketId: window.marketId,
        strategy: signal.strategy,
        side: signal.direction,
        tokenId,
        entryPrice,
        size,
        status: "open",
      };

      const id = this.db.openTrade(trade);
      trade.id = id;
      this.openCount++;

      const cost = (entryPrice * size).toFixed(2);
      log.info(
        `TRADE OPENED #${id}: ${trade.strategy} ${trade.side} @ ${trade.entryPrice.toFixed(3)} ` +
        `(${size} tokens, $${cost} cost) [spread pair]`,
      );

      this.emit("tradeOpened", trade);
      trades.push(trade);
    }

    return trades;
  }

  resolveWindow(window: MarketWindow, openPrice: number, closePrice: number): void {
    // BTC went UP if closePrice >= openPrice
    const outcome = closePrice >= openPrice ? "UP" : "DOWN";

    log.info(
      `Resolving window: ${window.question} | ` +
      `open=$${openPrice.toFixed(2)} close=$${closePrice.toFixed(2)} => ${outcome}`,
    );

    const trades = this.db.getOpenTradesForMarket(window.marketId);
    if (trades.length === 0) {
      log.info("No open trades to resolve for this window");
      this.db.resolveMarket(window.marketId, "resolved");
      return;
    }

    for (const trade of trades) {
      const won = trade.side === outcome;
      const settlementPrice = won ? 1.0 : 0.0;

      // P&L = (settlement - entry) * size
      const gross = (settlementPrice - trade.entryPrice) * trade.size;
      const entryCost = trade.entryPrice * trade.size;

      const result: TradeResult = {
        tradeId: trade.id!,
        exitPrice: settlementPrice,
        settlementPrice,
        pnl0pct: gross,
        pnl1pct: gross - entryCost * 0.01,
        pnl2pct: gross - entryCost * 0.02,
        pnl3pct: gross - entryCost * 0.03,
      };

      this.db.closeTrade(trade.id!, result);
      this.openCount = Math.max(0, this.openCount - 1);

      // Update bankroll (per-strategy tracking)
      this.bankroll.applyTradeResult(trade.id!, trade.entryPrice, trade.size, settlementPrice, trade.strategy);

      const emoji = won ? "WIN" : "LOSS";
      log.info(
        `TRADE RESOLVED #${trade.id}: ${emoji} ${trade.strategy} ${trade.side} | ` +
        `entry=${trade.entryPrice.toFixed(3)} settlement=${settlementPrice} | ` +
        `P&L(0%)=$${result.pnl0pct.toFixed(2)} P&L(3%)=$${result.pnl3pct.toFixed(2)}`,
      );

      this.emit("tradeResolved", trade, result);
    }

    this.db.resolveMarket(window.marketId, "resolved");
  }
}
