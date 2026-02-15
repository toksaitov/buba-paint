import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { Database } from "./db/database.js";
import type { BinanceFeed } from "./feeds/binance-feed.js";
import type { ClobFeed } from "./feeds/clob-feed.js";
import type { ChainlinkFeed } from "./feeds/chainlink-feed.js";

const log = createLogger("ticker");

export class TickLogger {
  private timer: ReturnType<typeof setInterval> | null = null;
  private tickCount = 0;

  constructor(
    private db: Database,
    private binanceFeed: BinanceFeed,
    private clobFeed: ClobFeed,
    private chainlinkFeed: ChainlinkFeed,
  ) {}

  start(): void {
    log.info(`Tick logger started (interval: ${CONFIG.TICK_INTERVAL}ms)`);
    this.timer = setInterval(() => this.sample(), CONFIG.TICK_INTERVAL);
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    log.info(`Tick logger stopped (${this.tickCount} ticks recorded)`);
  }

  private sample(): void {
    const binPrice = this.binanceFeed.getPrice();
    const bookState = this.clobFeed.getBookState();
    const clPrice = this.chainlinkFeed.getPrice();

    if (binPrice !== null) {
      this.db.logTick("binance", binPrice, null, null, null, null);
    }

    if (bookState.up) {
      this.db.logTick("clob_up", null,
        bookState.up.bestBid, bookState.up.bestAsk,
        bookState.up.bidSize, bookState.up.askSize);
    }

    if (bookState.down) {
      this.db.logTick("clob_down", null,
        bookState.down.bestBid, bookState.down.bestAsk,
        bookState.down.bidSize, bookState.down.askSize);
    }

    if (clPrice !== null) {
      this.db.logTick("chainlink", clPrice, null, null, null, null);
    }

    this.tickCount++;
    if (this.tickCount % 60 === 0) {
      log.debug(`Tick #${this.tickCount} | BTC=$${binPrice?.toFixed(2) ?? "N/A"} | CL=$${clPrice?.toFixed(2) ?? "N/A"}`);
    }
  }
}
