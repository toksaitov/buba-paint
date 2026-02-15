import { EventEmitter } from "node:events";
import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { Database } from "./db/database.js";
import type { Signal, SimulatedTrade, TradeResult, MarketWindow } from "./types.js";

const log = createLogger("positions");

export class PositionManager extends EventEmitter {
  private openCount = 0;

  constructor(private db: Database) {
    super();
  }

  tryOpen(signal: Signal, window: MarketWindow): SimulatedTrade | null {
    // Guard: max open positions
    if (this.openCount >= CONFIG.MAX_OPEN_POSITIONS) {
      log.debug("Max open positions reached, skipping signal");
      return null;
    }

    // Guard: check if we already have a position in this market + strategy + direction
    const existing = this.db.getOpenTradesForMarket(window.marketId);
    const duplicate = existing.find(
      (t) => t.strategy === signal.strategy && t.side === signal.direction,
    );
    if (duplicate) {
      log.debug(`Already have ${signal.strategy} ${signal.direction} position in ${window.marketId}`);
      return null;
    }

    const entryPrice = signal.direction === "UP" ? signal.upAsk : signal.downAsk;
    const tokenId = signal.direction === "UP" ? window.upTokenId : window.downTokenId;

    const trade: SimulatedTrade = {
      timestamp: signal.timestamp,
      marketId: window.marketId,
      strategy: signal.strategy,
      side: signal.direction,
      tokenId,
      entryPrice,
      size: CONFIG.POSITION_SIZE,
      status: "open",
    };

    const id = this.db.openTrade(trade);
    trade.id = id;
    this.openCount++;

    log.info(
      `TRADE OPENED #${id}: ${trade.strategy} ${trade.side} @ ${trade.entryPrice.toFixed(3)} ` +
      `($${trade.size} notional) [market: ${window.question}]`,
    );

    this.emit("tradeOpened", trade);
    return trade;
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
