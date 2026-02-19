import { CONFIG } from "../config.js";
import type { Strategy, StrategyContext, Signal, SignalDirection } from "../types.js";

export class LatencyArbStrategy implements Strategy {
  readonly name = "latency-arb";
  private lastSignalTime = 0;

  evaluate(ctx: StrategyContext): Signal | null {
    const { binancePrice, binanceMomentum, chainlinkPrice, bookState, windowTimeRemainingMs } = ctx;

    // Don't trade too close to expiry
    if (windowTimeRemainingMs < CONFIG.MIN_WINDOW_TIME_MS) return null;
    if (!bookState.up || !bookState.down) return null;

    // Cooldown: suppress signals for COOLDOWN_MS after the last one
    const now = Date.now();
    if (now - this.lastSignalTime < CONFIG.LATENCY_ARB_COOLDOWN_MS) return null;

    const upAsk = bookState.up.bestAsk;
    const downAsk = bookState.down.bestAsk;
    const upBid = bookState.up.bestBid;
    const downBid = bookState.down.bestBid;

    // Need valid prices
    if (upAsk <= 0 || downAsk <= 0) return null;

    let direction: SignalDirection | null = null;

    // Strong upward Binance momentum but UP token still hasn't repriced
    if (binanceMomentum > CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD && upAsk < CONFIG.LATENCY_ARB_MAX_ASK) {
      direction = "UP";
    }

    // Strong downward Binance momentum but DOWN token still hasn't repriced
    if (binanceMomentum < -CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD && downAsk < CONFIG.LATENCY_ARB_MAX_ASK) {
      direction = "DOWN";
    }

    if (!direction) return null;

    // Minimum entry price filter: cheap tokens (< 0.30) lost 100% in testing
    const entryAsk = direction === "UP" ? upAsk : downAsk;
    if (entryAsk < CONFIG.LATENCY_ARB_MIN_ASK) return null;

    // Confidence: how far above threshold the momentum is
    // Range [0.5, 1.0]: barely-above = 0.5, 3x threshold = 1.0
    const absMomentum = Math.abs(binanceMomentum);
    const ratio = absMomentum / CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD;
    const confidence = Math.min(1.0, 0.25 + 0.25 * ratio);

    this.lastSignalTime = now;

    return {
      timestamp: now,
      strategy: this.name,
      direction,
      confidence,
      binancePrice,
      chainlinkPrice: chainlinkPrice ?? 0,
      upAsk,
      downAsk,
      upBid,
      downBid,
      metadata: {
        momentum: binanceMomentum,
        momentumThreshold: CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD,
        maxAsk: CONFIG.LATENCY_ARB_MAX_ASK,
        minAsk: CONFIG.LATENCY_ARB_MIN_ASK,
        confidence,
        cooldownMs: CONFIG.LATENCY_ARB_COOLDOWN_MS,
        windowTimeRemainingMs,
      },
    };
  }
}
