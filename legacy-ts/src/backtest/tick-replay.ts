/**
 * Reads tick_data from the merged market-data.db and yields tick groups
 * ordered by timestamp. Each group contains all sources sampled at the
 * same second.
 */

import type BetterSqlite3 from "better-sqlite3";

export interface TickSample {
  price: number | null;
  bid: number | null;
  ask: number | null;
  bidSize: number | null;
  askSize: number | null;
}

export interface TickGroup {
  timestamp: number;
  binance: TickSample | null;
  chainlink: TickSample | null;
  clobUp: TickSample | null;
  clobDown: TickSample | null;
}

interface RawTick {
  timestamp: number;
  source: string;
  price: number | null;
  bid: number | null;
  ask: number | null;
  bid_size: number | null;
  ask_size: number | null;
}

export class TickReplay {
  private ticks: RawTick[];
  private cursor = 0;

  constructor(db: BetterSqlite3.Database, startTime: number, endTime: number);
  constructor(cachedTicks: RawTick[]);
  constructor(dbOrTicks: BetterSqlite3.Database | RawTick[], startTime?: number, endTime?: number) {
    if (Array.isArray(dbOrTicks)) {
      this.ticks = dbOrTicks;
    } else {
      // Load all ticks into memory for fast replay
      this.ticks = dbOrTicks
        .prepare(
          `SELECT timestamp, source, price, bid, ask, bid_size, ask_size
           FROM tick_data
           WHERE timestamp >= ? AND timestamp <= ?
           ORDER BY timestamp`,
        )
        .all(startTime!, endTime!) as RawTick[];
    }
  }

  /** Load raw ticks from DB (for caching across sweep runs). */
  static loadTicks(db: BetterSqlite3.Database, startTime: number, endTime: number): RawTick[] {
    return db
      .prepare(
        `SELECT timestamp, source, price, bid, ask, bid_size, ask_size
         FROM tick_data
         WHERE timestamp >= ? AND timestamp <= ?
         ORDER BY timestamp`,
      )
      .all(startTime, endTime) as RawTick[];
  }

  get totalTicks(): number {
    return this.ticks.length;
  }

  reset(): void {
    this.cursor = 0;
  }

  /** Yield tick groups one at a time. Returns null when exhausted. */
  next(): TickGroup | null {
    if (this.cursor >= this.ticks.length) return null;

    const ts = this.ticks[this.cursor].timestamp;
    const group: TickGroup = {
      timestamp: ts,
      binance: null,
      chainlink: null,
      clobUp: null,
      clobDown: null,
    };

    // Consume all ticks within 10ms of the first tick's timestamp.
    // The tick logger calls db.logTick() sequentially for each source,
    // so Date.now() can advance 1-2ms between clob_up and clob_down
    // within the same 1-second sample. Without this tolerance, the
    // TickReplay would split them into separate groups, causing
    // feedState to mix a NEW value from one CLOB side with a STALE
    // value from the other side — creating artificial spread signals.
    while (this.cursor < this.ticks.length && this.ticks[this.cursor].timestamp - ts <= 10) {
      const t = this.ticks[this.cursor];
      const sample: TickSample = {
        price: t.price,
        bid: t.bid,
        ask: t.ask,
        bidSize: t.bid_size,
        askSize: t.ask_size,
      };

      switch (t.source) {
        case "binance":
          group.binance = sample;
          break;
        case "chainlink":
          group.chainlink = sample;
          break;
        case "clob_up":
          group.clobUp = sample;
          break;
        case "clob_down":
          group.clobDown = sample;
          break;
      }

      this.cursor++;
    }

    return group;
  }
}
