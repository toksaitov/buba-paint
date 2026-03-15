import { createLogger } from "./utils/logger.js";

const log = createLogger("regime");

export type Regime = "trending" | "choppy" | "unknown";

/**
 * Experimental regime detector.
 * Tracks 1-minute returns over a rolling 2-hour window and classifies
 * the current market regime based on direction-reversal frequency.
 *
 * - Choppy: >65% of consecutive minutes reverse direction
 * - Trending: <35% reversal rate (strong directional continuation)
 * - Unknown: insufficient data or ambiguous
 *
 * Default OFF — enable via REGIME_DETECTION_ENABLED=true
 */
export class RegimeDetector {
  private minuteReturns: number[] = [];
  private lastMinutePrice: number | null = null;
  private lastMinute = 0;
  private readonly MAX_MINUTES = 120; // 2-hour window

  addPrice(price: number, timestamp: number): void {
    const minute = Math.floor(timestamp / 60_000);
    if (minute === this.lastMinute) return; // one sample per minute

    if (this.lastMinutePrice !== null) {
      const ret = (price - this.lastMinutePrice) / this.lastMinutePrice;
      this.minuteReturns.push(ret);
      if (this.minuteReturns.length > this.MAX_MINUTES) {
        this.minuteReturns.shift();
      }
    }

    this.lastMinutePrice = price;
    this.lastMinute = minute;
  }

  getRegime(): Regime {
    if (this.minuteReturns.length < 30) return "unknown";

    // Count direction reversals (sign changes between consecutive minutes)
    let reversals = 0;
    for (let i = 1; i < this.minuteReturns.length; i++) {
      const prev = Math.sign(this.minuteReturns[i - 1]);
      const curr = Math.sign(this.minuteReturns[i]);
      // Skip zero-return minutes
      if (prev !== 0 && curr !== 0 && prev !== curr) {
        reversals++;
      }
    }

    const reversalRate = reversals / (this.minuteReturns.length - 1);

    if (reversalRate > 0.65) return "choppy";
    if (reversalRate < 0.35) return "trending";
    return "unknown";
  }

  /** Realized volatility: annualized std dev of 1-minute returns */
  getRealizedVol(): number {
    if (this.minuteReturns.length < 10) return 0;

    const mean = this.minuteReturns.reduce((a, b) => a + b, 0) / this.minuteReturns.length;
    const variance =
      this.minuteReturns.reduce((sum, r) => sum + (r - mean) ** 2, 0) /
      (this.minuteReturns.length - 1);
    const stdDev = Math.sqrt(variance);

    // Annualize: sqrt(minutes per year) ≈ sqrt(525600)
    return stdDev * Math.sqrt(525_600);
  }
}
