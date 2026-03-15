import { CONFIG } from "../config.js";
import { now } from "../clock.js";
import type { Strategy, StrategyContext, Signal, SignalDirection } from "../types.js";

export class LatencyArbStrategy implements Strategy {
  readonly name = "latency-arb";
  private lastSignalTime = 0;

  // Adaptive momentum threshold (P3-A): rolling buffer of recent momentum magnitudes
  private momentumBuffer: number[] = [];
  private readonly MOMENTUM_BUFFER_SIZE = 1800; // ~30 min at 1 sample/s
  private adaptiveThreshold = CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD;
  private lastThresholdCalc = 0;

  evaluate(ctx: StrategyContext): Signal | null {
    const { binancePrice, binanceMomentum, chainlinkPrice, bookState, windowTimeRemainingMs } = ctx;

    // Record momentum magnitude for adaptive threshold
    this.momentumBuffer.push(Math.abs(binanceMomentum));
    if (this.momentumBuffer.length > this.MOMENTUM_BUFFER_SIZE) {
      this.momentumBuffer.shift();
    }

    // Don't trade too close to expiry
    if (windowTimeRemainingMs < CONFIG.MIN_WINDOW_TIME_MS) return null;
    if (!bookState.up || !bookState.down) return null;

    // Cooldown: suppress signals for COOLDOWN_MS after the last one
    const t = now();
    if (t - this.lastSignalTime < CONFIG.LATENCY_ARB_COOLDOWN_MS) return null;

    const upAsk = bookState.up.bestAsk;
    const downAsk = bookState.down.bestAsk;
    const upBid = bookState.up.bestBid;
    const downBid = bookState.down.bestBid;

    // Need valid prices
    if (upAsk <= 0 || downAsk <= 0) return null;

    // Use adaptive threshold: max(static config, 85th percentile of recent momentum)
    const effectiveThreshold = this.getAdaptiveThreshold(t);

    let direction: SignalDirection | null = null;

    // Strong upward Binance momentum but UP token still hasn't repriced
    if (binanceMomentum > effectiveThreshold && upAsk < CONFIG.LATENCY_ARB_MAX_ASK) {
      direction = "UP";
    }

    // Strong downward Binance momentum but DOWN token still hasn't repriced
    if (binanceMomentum < -effectiveThreshold && downAsk < CONFIG.LATENCY_ARB_MAX_ASK) {
      direction = "DOWN";
    }

    if (!direction) return null;

    // Minimum entry price filter: cheap tokens (< 0.30) lost 100% in testing
    const entryAsk = direction === "UP" ? upAsk : downAsk;
    if (entryAsk < CONFIG.LATENCY_ARB_MIN_ASK) return null;

    // Confidence: how far above threshold the momentum is
    // Range [0.70, 1.0]: 1x threshold = 0.70, 2x threshold = 1.0
    const absMomentum = Math.abs(binanceMomentum);
    const ratio = absMomentum / effectiveThreshold;
    const confidence = Math.min(1.0, 0.40 + 0.30 * ratio);

    this.lastSignalTime = t;

    return {
      timestamp: t,
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
        momentumThreshold: effectiveThreshold,
        staticThreshold: CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD,
        maxAsk: CONFIG.LATENCY_ARB_MAX_ASK,
        minAsk: CONFIG.LATENCY_ARB_MIN_ASK,
        confidence,
        cooldownMs: CONFIG.LATENCY_ARB_COOLDOWN_MS,
        windowTimeRemainingMs,
      },
    };
  }

  /** Compute adaptive threshold: max(static, 85th percentile of recent momentum). Cached for 10s. */
  private getAdaptiveThreshold(t: number): number {
    if (t - this.lastThresholdCalc < 10_000) return this.adaptiveThreshold;
    this.lastThresholdCalc = t;

    // Need at least 60 samples (~1 minute) before adapting
    if (this.momentumBuffer.length < 60) {
      this.adaptiveThreshold = CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD;
      return this.adaptiveThreshold;
    }

    const sorted = [...this.momentumBuffer].sort((a, b) => a - b);
    const p85 = sorted[Math.floor(sorted.length * 0.85)];
    this.adaptiveThreshold = Math.max(CONFIG.LATENCY_ARB_MOMENTUM_THRESHOLD, p85);
    return this.adaptiveThreshold;
  }
}
