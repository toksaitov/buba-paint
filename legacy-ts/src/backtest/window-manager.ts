/**
 * Manages market window lifecycle from pre-computed data.
 * Replaces MarketDiscovery during backtesting.
 */

import type BetterSqlite3 from "better-sqlite3";
import type { MarketWindow } from "../types.js";

export interface MarketSettlement {
  marketId: string;
  question: string;
  upTokenId: string;
  downTokenId: string;
  conditionId: string;
  slug: string;
  startTime: number;
  endTime: number;
  openPrice: number;
  closePrice: number;
  outcome: "UP" | "DOWN";
}

interface RawMarket {
  market_id: string;
  question: string;
  up_token_id: string;
  down_token_id: string;
  condition_id: string;
  slug: string;
  start_time: number;
  end_time: number;
  open_price: number;
  close_price: number;
  outcome: string;
}

export class WindowManager {
  private windows: MarketSettlement[];
  private cursor = 0;
  current: MarketSettlement | null = null;

  constructor(db: BetterSqlite3.Database, startTime: number, endTime: number) {
    const rows = db
      .prepare(
        `SELECT market_id, question, up_token_id, down_token_id,
                condition_id, slug, start_time, end_time,
                open_price, close_price, outcome
         FROM markets
         WHERE end_time >= ? AND start_time <= ?
           AND outcome IS NOT NULL
         ORDER BY start_time`,
      )
      .all(startTime, endTime) as RawMarket[];

    this.windows = rows.map((r) => ({
      marketId: r.market_id,
      question: r.question,
      upTokenId: r.up_token_id,
      downTokenId: r.down_token_id,
      conditionId: r.condition_id,
      slug: r.slug,
      startTime: r.start_time,
      endTime: r.end_time,
      openPrice: r.open_price,
      closePrice: r.close_price,
      outcome: r.outcome as "UP" | "DOWN",
    }));
  }

  get totalWindows(): number {
    return this.windows.length;
  }

  /**
   * Advance the window state based on the current timestamp.
   * Returns events that occurred at this timestamp.
   */
  advance(timestamp: number): { opened?: MarketSettlement; closed?: MarketSettlement } {
    const result: { opened?: MarketSettlement; closed?: MarketSettlement } = {};

    // Check if current window closed
    if (this.current && timestamp >= this.current.endTime) {
      result.closed = this.current;
      this.current = null;
    }

    // Check if next window opened (skip any windows already expired)
    while (!this.current && this.cursor < this.windows.length) {
      const next = this.windows[this.cursor];
      if (timestamp >= next.endTime) {
        // Window already ended — skip it
        this.cursor++;
        continue;
      }
      if (timestamp >= next.startTime) {
        this.current = next;
        this.cursor++;
        result.opened = this.current;
      }
      break;
    }

    return result;
  }

  /** Convert a MarketSettlement to a MarketWindow (for PositionManager compatibility). */
  toMarketWindow(settlement: MarketSettlement): MarketWindow {
    return {
      marketId: settlement.marketId,
      question: settlement.question,
      upTokenId: settlement.upTokenId,
      downTokenId: settlement.downTokenId,
      conditionId: settlement.conditionId,
      slug: settlement.slug,
      startTime: settlement.startTime,
      endTime: settlement.endTime,
    };
  }

  reset(): void {
    this.cursor = 0;
    this.current = null;
  }
}
