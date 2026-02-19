import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { SignalDirection } from "./types.js";

const log = createLogger("trend");

interface Outcome {
  direction: SignalDirection;
  won: boolean;
  timestamp: number;
}

export class TrendTracker {
  private recentOutcomes: Outcome[] = [];

  recordOutcome(direction: SignalDirection, won: boolean): void {
    this.recentOutcomes.push({ direction, won, timestamp: Date.now() });
    if (this.recentOutcomes.length > CONFIG.TREND_FILTER_WINDOW) {
      this.recentOutcomes.shift();
    }
  }

  /**
   * Returns bias: positive = UP favored, negative = DOWN favored, 0 = neutral.
   */
  getTrendBias(): number {
    if (this.recentOutcomes.length < 3) return 0;

    let upWins = 0, upTotal = 0, downWins = 0, downTotal = 0;
    for (const o of this.recentOutcomes) {
      if (o.direction === "UP") {
        upTotal++;
        if (o.won) upWins++;
      } else {
        downTotal++;
        if (o.won) downWins++;
      }
    }

    const upRate = upTotal > 0 ? upWins / upTotal : 0.5;
    const downRate = downTotal > 0 ? downWins / downTotal : 0.5;
    return upRate - downRate;
  }

  shouldSuppress(direction: SignalDirection): boolean {
    if (!CONFIG.TREND_FILTER_ENABLED) return false;

    const bias = this.getTrendBias();

    if (direction === "UP" && bias < -CONFIG.TREND_FILTER_THRESHOLD) {
      log.info(`Suppressing UP signal: trend bias ${bias.toFixed(2)} (DOWN favored)`);
      return true;
    }
    if (direction === "DOWN" && bias > CONFIG.TREND_FILTER_THRESHOLD) {
      log.info(`Suppressing DOWN signal: trend bias ${bias.toFixed(2)} (UP favored)`);
      return true;
    }

    return false;
  }
}
