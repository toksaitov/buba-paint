import { CONFIG } from "../config.js";
import { BaseFeed } from "./base-feed.js";
import type { ChainlinkTick } from "../types.js";

export class ChainlinkFeed extends BaseFeed {
  private latestTick: ChainlinkTick | null = null;
  private lastUpdateAt = 0;
  private staleTimer: ReturnType<typeof setInterval> | null = null;
  private staleWarned = false;

  constructor() {
    super("chainlink", CONFIG.RTDS_WS_URL, CONFIG.RTDS_PING_INTERVAL);
  }

  protected onConnected(): void {
    // Reset staleness tracking — give the new connection a fresh window
    // to receive data before being considered stale again
    this.lastUpdateAt = Date.now();
    this.staleWarned = false;

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
    this.startStaleCheck();
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
    this.lastUpdateAt = Date.now();
    this.staleWarned = false;
    this.emit("tick", tick);
    this.log.debug(`Chainlink BTC/USD: $${tick.value.toFixed(2)}`);
  }

  /**
   * Returns the latest price, or null if the feed is stale.
   */
  getPrice(): number | null {
    if (this.isStale()) return null;
    return this.latestTick?.value ?? null;
  }

  getTimestamp(): number | null {
    return this.latestTick?.timestamp ?? null;
  }

  isStale(): boolean {
    if (this.lastUpdateAt === 0) return false; // haven't connected yet
    return Date.now() - this.lastUpdateAt > CONFIG.CHAINLINK_STALE_MS;
  }

  private startStaleCheck(): void {
    this.stopStaleCheck();
    // Check every 10 seconds
    this.staleTimer = setInterval(() => {
      if (!this.isStale()) return;

      if (!this.staleWarned) {
        const ageSec = ((Date.now() - this.lastUpdateAt) / 1000).toFixed(0);
        this.log.warn(
          `Feed stale: no update for ${ageSec}s (threshold: ${CONFIG.CHAINLINK_STALE_MS / 1000}s) — forcing reconnect`,
        );
        this.staleWarned = true;
        this.emit("stale");
        this.forceReconnect();
      }
    }, 10_000);
  }

  private stopStaleCheck(): void {
    if (this.staleTimer) {
      clearInterval(this.staleTimer);
      this.staleTimer = null;
    }
  }

  private forceReconnect(): void {
    this.disconnect();
    this.connect();
  }

  disconnect(): void {
    this.stopStaleCheck();
    super.disconnect();
  }
}
