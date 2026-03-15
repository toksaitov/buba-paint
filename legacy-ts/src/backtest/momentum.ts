/**
 * Rolling window momentum calculator.
 * Replicates BinanceFeed.getMomentum() exactly — see src/feeds/binance-feed.ts:50-55.
 */

interface PricePoint {
  price: number;
  timestamp: number;
}

export class MomentumCalculator {
  private window: PricePoint[] = [];
  private windowMs: number;

  constructor(windowMs: number) {
    this.windowMs = windowMs;
  }

  push(price: number, timestamp: number): void {
    this.window.push({ price, timestamp });
    const cutoff = timestamp - this.windowMs;
    while (this.window.length > 0 && this.window[0].timestamp < cutoff) {
      this.window.shift();
    }
  }

  get(): number {
    if (this.window.length < 2) return 0;
    const oldest = this.window[0];
    const latest = this.window[this.window.length - 1];
    return (latest.price - oldest.price) / oldest.price;
  }

  reset(): void {
    this.window = [];
  }
}
