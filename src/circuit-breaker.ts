import { CONFIG } from "./config.js";
import { now as clockNow } from "./clock.js";
import { createLogger } from "./utils/logger.js";

const log = createLogger("circuit-breaker");

export class CircuitBreaker {
  private consecutiveLosses = 0;
  private pauseUntil = 0;

  recordResult(won: boolean): void {
    if (won) {
      this.consecutiveLosses = 0;
      return;
    }

    this.consecutiveLosses++;
    if (this.consecutiveLosses >= CONFIG.CIRCUIT_BREAKER_LOSSES) {
      const pauseMs = CONFIG.CIRCUIT_BREAKER_PAUSE_MS;
      this.pauseUntil = clockNow() + pauseMs;
      log.warn(
        `Circuit breaker triggered: ${this.consecutiveLosses} consecutive losses. ` +
        `Pausing for ${(pauseMs / 60_000).toFixed(0)} minutes.`,
      );
      this.consecutiveLosses = 0;
    }
  }

  canTrade(): boolean {
    return clockNow() >= this.pauseUntil;
  }

  isPaused(): boolean {
    return clockNow() < this.pauseUntil;
  }

  getConsecutiveLosses(): number {
    return this.consecutiveLosses;
  }
}
