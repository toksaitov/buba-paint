/**
 * Maintains current market state from replayed ticks.
 * Replaces live feed classes during backtesting.
 */

import type { BookState, TopOfBook } from "../types.js";
import type { TickGroup } from "./tick-replay.js";

export class FeedState {
  binancePrice: number | null = null;
  chainlinkPrice: number | null = null;
  bookState: BookState = { up: null, down: null };

  update(group: TickGroup): void {
    if (group.binance?.price != null) {
      this.binancePrice = group.binance.price;
    }

    if (group.chainlink?.price != null) {
      this.chainlinkPrice = group.chainlink.price;
    }

    if (group.clobUp && group.clobUp.bid != null && group.clobUp.ask != null) {
      this.bookState.up = {
        bestBid: group.clobUp.bid,
        bestAsk: group.clobUp.ask,
        bidSize: group.clobUp.bidSize ?? 0,
        askSize: group.clobUp.askSize ?? 0,
        timestamp: group.timestamp,
      };
    }

    if (group.clobDown && group.clobDown.bid != null && group.clobDown.ask != null) {
      this.bookState.down = {
        bestBid: group.clobDown.bid,
        bestAsk: group.clobDown.ask,
        bidSize: group.clobDown.bidSize ?? 0,
        askSize: group.clobDown.askSize ?? 0,
        timestamp: group.timestamp,
      };
    }
  }

  reset(): void {
    this.binancePrice = null;
    this.chainlinkPrice = null;
    this.bookState = { up: null, down: null };
  }
}
