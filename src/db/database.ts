import BetterSqlite3 from "better-sqlite3";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { runMigrations } from "./schema.js";
import type { Signal, SimulatedTrade, TradeResult, MarketWindow } from "../types.js";
import { createLogger } from "../utils/logger.js";

const log = createLogger("db");

export class Database {
  private db: BetterSqlite3.Database;

  private stmtInsertTick: BetterSqlite3.Statement;
  private stmtInsertMarket: BetterSqlite3.Statement;
  private stmtInsertSignal: BetterSqlite3.Statement;
  private stmtInsertTrade: BetterSqlite3.Statement;
  private stmtInsertResult: BetterSqlite3.Statement;
  private stmtUpdateTradeStatus: BetterSqlite3.Statement;
  private stmtUpdateMarketStatus: BetterSqlite3.Statement;
  private stmtGetOpenTrades: BetterSqlite3.Statement;
  private stmtGetMarketBySlug: BetterSqlite3.Statement;

  constructor(dbPath: string) {
    mkdirSync(dirname(dbPath), { recursive: true });
    this.db = new BetterSqlite3(dbPath);
    this.db.pragma("journal_mode = WAL");
    this.db.pragma("synchronous = NORMAL");
    runMigrations(this.db);
    log.info(`Database initialized at ${dbPath}`);

    this.stmtInsertTick = this.db.prepare(
      `INSERT INTO tick_data (timestamp, source, price, bid, ask, bid_size, ask_size)
       VALUES (?, ?, ?, ?, ?, ?, ?)`
    );

    this.stmtInsertMarket = this.db.prepare(
      `INSERT OR IGNORE INTO markets (market_id, question, condition_id, slug, up_token_id, down_token_id, start_time, end_time)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
    );

    this.stmtInsertSignal = this.db.prepare(
      `INSERT INTO signals (timestamp, strategy, direction, binance_price, chainlink_price, up_ask, down_ask, up_bid, down_bid, metadata)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
    );

    this.stmtInsertTrade = this.db.prepare(
      `INSERT INTO simulated_trades (timestamp, market_id, strategy, side, token_id, entry_price, size, status)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
    );

    this.stmtInsertResult = this.db.prepare(
      `INSERT INTO trade_results (trade_id, exit_price, settlement_price, pnl_0pct, pnl_1pct, pnl_2pct, pnl_3pct, resolved_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
    );

    this.stmtUpdateTradeStatus = this.db.prepare(
      `UPDATE simulated_trades SET status = ? WHERE id = ?`
    );

    this.stmtUpdateMarketStatus = this.db.prepare(
      `UPDATE markets SET status = ? WHERE market_id = ?`
    );

    this.stmtGetOpenTrades = this.db.prepare(
      `SELECT * FROM simulated_trades WHERE market_id = ? AND status = 'open'`
    );

    this.stmtGetMarketBySlug = this.db.prepare(
      `SELECT * FROM markets WHERE slug = ?`
    );
  }

  logTick(
    source: string,
    price: number | null,
    bid: number | null,
    ask: number | null,
    bidSize: number | null,
    askSize: number | null,
  ): void {
    this.stmtInsertTick.run(Date.now(), source, price, bid, ask, bidSize, askSize);
  }

  upsertMarket(window: MarketWindow): void {
    this.stmtInsertMarket.run(
      window.marketId, window.question, window.conditionId, window.slug,
      window.upTokenId, window.downTokenId, window.startTime, window.endTime,
    );
  }

  logSignal(signal: Signal): void {
    this.stmtInsertSignal.run(
      signal.timestamp, signal.strategy, signal.direction,
      signal.binancePrice, signal.chainlinkPrice,
      signal.upAsk, signal.downAsk, signal.upBid, signal.downBid,
      JSON.stringify(signal.metadata),
    );
  }

  openTrade(trade: SimulatedTrade): number {
    const result = this.stmtInsertTrade.run(
      trade.timestamp, trade.marketId, trade.strategy, trade.side,
      trade.tokenId, trade.entryPrice, trade.size, trade.status,
    );
    return Number(result.lastInsertRowid);
  }

  closeTrade(tradeId: number, result: TradeResult): void {
    this.db.transaction(() => {
      this.stmtUpdateTradeStatus.run("closed", tradeId);
      this.stmtInsertResult.run(
        result.tradeId, result.exitPrice, result.settlementPrice,
        result.pnl0pct, result.pnl1pct, result.pnl2pct, result.pnl3pct,
        Date.now(),
      );
    })();
  }

  getOpenTradesForMarket(marketId: string): SimulatedTrade[] {
    const rows = this.stmtGetOpenTrades.all(marketId) as Record<string, unknown>[];
    return rows.map((row) => ({
      id: row.id as number,
      timestamp: row.timestamp as number,
      marketId: row.market_id as string,
      strategy: row.strategy as string,
      side: row.side as SimulatedTrade["side"],
      tokenId: row.token_id as string,
      entryPrice: row.entry_price as number,
      size: row.size as number,
      status: row.status as SimulatedTrade["status"],
    }));
  }

  resolveMarket(marketId: string, status: string): void {
    this.stmtUpdateMarketStatus.run(status, marketId);
  }

  close(): void {
    this.db.close();
    log.info("Database closed");
  }
}
