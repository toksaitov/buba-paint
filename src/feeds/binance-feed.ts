import { CONFIG } from "../config.js";
import { BaseFeed } from "./base-feed.js";
import type { BinanceTick } from "../types.js";

interface PricePoint {
  price: number;
  timestamp: number;
}

export class BinanceFeed extends BaseFeed {
  private latestPrice: number | null = null;
  private priceWindow: PricePoint[] = [];

  constructor() {
    super("binance", CONFIG.BINANCE_WS_URL, null);
  }

  protected onMessage(data: string): void {
    const raw = JSON.parse(data);

    // Binance aggTrade format:
    // { "e":"aggTrade", "E":ts, "s":"BTCUSDT", "p":"97234.50", "q":"0.001", "T":tradeTs, ... }
    if (raw.e !== "aggTrade") return;

    const tick: BinanceTick = {
      eventTime: raw.E,
      price: parseFloat(raw.p),
      quantity: parseFloat(raw.q),
      tradeTime: raw.T,
    };

    this.latestPrice = tick.price;

    const now = Date.now();
    this.priceWindow.push({ price: tick.price, timestamp: now });

    // Prune entries older than the momentum window
    const cutoff = now - CONFIG.MOMENTUM_WINDOW_MS;
    while (this.priceWindow.length > 0 && this.priceWindow[0].timestamp < cutoff) {
      this.priceWindow.shift();
    }

    this.emit("tick", tick);
  }

  getPrice(): number | null {
    return this.latestPrice;
  }

  getMomentum(): number {
    if (this.priceWindow.length < 2) return 0;
    const oldest = this.priceWindow[0];
    const latest = this.priceWindow[this.priceWindow.length - 1];
    return (latest.price - oldest.price) / oldest.price;
  }
}
