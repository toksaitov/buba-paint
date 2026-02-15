import { CONFIG } from "../config.js";
import type { Strategy, StrategyContext, Signal } from "../types.js";

export class SpreadCaptureStrategy implements Strategy {
  readonly name = "spread-capture";

  evaluate(ctx: StrategyContext): Signal[] | null {
    const { binancePrice, chainlinkPrice, bookState } = ctx;

    if (!bookState.up || !bookState.down) return null;

    const upAsk = bookState.up.bestAsk;
    const downAsk = bookState.down.bestAsk;
    const upBid = bookState.up.bestBid;
    const downBid = bookState.down.bestBid;

    // Need valid prices
    if (upAsk <= 0 || downAsk <= 0) return null;

    // Reject degenerate/empty book sides
    if (upAsk < CONFIG.SPREAD_CAPTURE_MIN_ASK || downAsk < CONFIG.SPREAD_CAPTURE_MIN_ASK) return null;

    const totalAsk = upAsk + downAsk;

    // If sum of asks is at or above threshold, no spread to capture
    if (totalAsk >= CONFIG.SPREAD_CAPTURE_THRESHOLD) return null;

    const now = Date.now();
    const sharedMeta = {
      totalAsk,
      threshold: CONFIG.SPREAD_CAPTURE_THRESHOLD,
      spreadEdge: 1.0 - totalAsk,
    };

    // True arbitrage: buy BOTH sides for guaranteed profit at settlement
    return [
      {
        timestamp: now,
        strategy: this.name,
        direction: "UP" as const,
        binancePrice,
        chainlinkPrice: chainlinkPrice ?? 0,
        upAsk,
        downAsk,
        upBid,
        downBid,
        metadata: sharedMeta,
      },
      {
        timestamp: now,
        strategy: this.name,
        direction: "DOWN" as const,
        binancePrice,
        chainlinkPrice: chainlinkPrice ?? 0,
        upAsk,
        downAsk,
        upBid,
        downBid,
        metadata: sharedMeta,
      },
    ];
  }
}
