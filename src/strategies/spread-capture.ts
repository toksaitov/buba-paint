import { CONFIG } from "../config.js";
import type { Strategy, StrategyContext, Signal, SignalDirection } from "../types.js";

export class SpreadCaptureStrategy implements Strategy {
  readonly name = "spread-capture";

  evaluate(ctx: StrategyContext): Signal | null {
    const { binancePrice, chainlinkPrice, bookState } = ctx;

    if (!bookState.up || !bookState.down) return null;

    const upAsk = bookState.up.bestAsk;
    const downAsk = bookState.down.bestAsk;
    const upBid = bookState.up.bestBid;
    const downBid = bookState.down.bestBid;

    // Need valid prices
    if (upAsk <= 0 || downAsk <= 0) return null;

    const totalAsk = upAsk + downAsk;

    // If sum of asks is below threshold, there's spread to capture
    if (totalAsk >= CONFIG.SPREAD_CAPTURE_THRESHOLD) return null;

    // Buy the cheaper side (more edge)
    const direction: SignalDirection = upAsk <= downAsk ? "UP" : "DOWN";

    return {
      timestamp: Date.now(),
      strategy: this.name,
      direction,
      binancePrice,
      chainlinkPrice: chainlinkPrice ?? 0,
      upAsk,
      downAsk,
      upBid,
      downBid,
      metadata: {
        totalAsk,
        threshold: CONFIG.SPREAD_CAPTURE_THRESHOLD,
        spreadEdge: 1.0 - totalAsk,
      },
    };
  }
}
