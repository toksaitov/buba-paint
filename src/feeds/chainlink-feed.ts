import { CONFIG } from "../config.js";
import { BaseFeed } from "./base-feed.js";
import type { ChainlinkTick } from "../types.js";

export class ChainlinkFeed extends BaseFeed {
  private latestTick: ChainlinkTick | null = null;

  constructor() {
    super("chainlink", CONFIG.RTDS_WS_URL, CONFIG.RTDS_PING_INTERVAL);
  }

  protected onConnected(): void {
    const msg = {
      action: "subscribe",
      subscriptions: [
        {
          topic: "crypto_prices_chainlink",
          type: "*",
          filters: JSON.stringify({ symbol: "btc/usd" }),
        },
      ],
    };
    this.log.info("Subscribing to crypto_prices_chainlink btc/usd");
    this.send(msg);
  }

  protected onMessage(data: string): void {
    if (!data || data.trim() === "") return;

    const raw = JSON.parse(data);

    if (Array.isArray(raw)) {
      for (const item of raw) {
        this.processMessage(item);
      }
      return;
    }

    this.processMessage(raw);
  }

  private processMessage(msg: Record<string, unknown>): void {
    // Regular update: { topic: "crypto_prices_chainlink", payload: { symbol, timestamp, value } }
    if (msg.topic === "crypto_prices_chainlink") {
      const payload = msg.payload as Record<string, unknown> | undefined;
      if (!payload) return;
      this.handleTick(payload);
      return;
    }

    // Initial data dump: { payload: { data: [{ timestamp, value }, ...] } }
    // Sent on first connection without a topic field
    const payload = msg.payload as Record<string, unknown> | undefined;
    if (payload?.data && Array.isArray(payload.data)) {
      const entries = payload.data as Array<Record<string, unknown>>;
      if (entries.length > 0) {
        // Take the most recent entry from the initial dump
        const latest = entries[entries.length - 1];
        this.handleTick({ symbol: "btc/usd", ...latest });
        this.log.info(`Initial data dump: ${entries.length} entries, latest $${(latest.value as number)?.toFixed(2)}`);
      }
      return;
    }
  }

  private handleTick(payload: Record<string, unknown>): void {
    const value = typeof payload.value === "string"
      ? parseFloat(payload.value as string)
      : (payload.value as number);

    if (isNaN(value) || value <= 0) return;

    const tick: ChainlinkTick = {
      symbol: (payload.symbol as string) ?? "btc/usd",
      timestamp: payload.timestamp as number,
      value,
    };

    this.latestTick = tick;
    this.emit("tick", tick);
    this.log.debug(`Chainlink BTC/USD: $${tick.value.toFixed(2)}`);
  }

  getPrice(): number | null {
    return this.latestTick?.value ?? null;
  }

  getTimestamp(): number | null {
    return this.latestTick?.timestamp ?? null;
  }
}
