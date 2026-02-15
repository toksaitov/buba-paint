import { CONFIG } from "../config.js";
import { BaseFeed } from "./base-feed.js";
import type { BookState, TopOfBook, OrderLevel } from "../types.js";

export class ClobFeed extends BaseFeed {
  private bookState: BookState = { up: null, down: null };
  private upTokenId: string | null = null;
  private downTokenId: string | null = null;

  constructor() {
    super("clob", CONFIG.CLOB_WS_URL, CONFIG.CLOB_PING_INTERVAL);
  }

  resubscribe(upTokenId: string, downTokenId: string): void {
    this.upTokenId = upTokenId;
    this.downTokenId = downTokenId;
    this.bookState = { up: null, down: null };

    if (this.getStatus() === "connected") {
      this.sendSubscription();
    } else {
      this.connect();
    }
  }

  protected onConnected(): void {
    if (this.upTokenId && this.downTokenId) {
      this.sendSubscription();
    }
  }

  private sendSubscription(): void {
    const msg = {
      type: "market",
      assets_ids: [this.upTokenId, this.downTokenId],
    };
    this.log.info("Subscribing to market", msg);
    this.send(msg);
  }

  protected onMessage(data: string): void {
    const raw = JSON.parse(data);

    if (Array.isArray(raw)) {
      // Batch message — process each item
      for (const item of raw) {
        this.handleEvent(item);
      }
    } else {
      this.handleEvent(raw);
    }
  }

  private handleEvent(event: Record<string, unknown>): void {
    const eventType = event.event_type as string | undefined;

    if (eventType === "price_change") {
      this.handlePriceChange(event);
    } else if (eventType === "last_trade_price") {
      this.log.debug("Last trade price", event);
    } else if (event.asset_id && (event.bids || event.asks)) {
      // Initial book snapshot (no event_type field) or explicit book event
      this.handleBook(event);
    }
  }

  private handleBook(event: Record<string, unknown>): void {
    const assetId = event.asset_id as string;
    const bids = this.parseLevels(event.bids || event.buys, false);  // descending: highest bid first
    const asks = this.parseLevels(event.asks || event.sells, true);  // ascending: lowest ask first
    const timestamp = this.parseTimestamp(event.timestamp);

    const tob: TopOfBook = {
      bestBid: bids.length > 0 ? bids[0].price : 0,
      bestAsk: asks.length > 0 ? asks[0].price : 0,
      bidSize: bids.length > 0 ? bids[0].size : 0,
      askSize: asks.length > 0 ? asks[0].size : 0,
      timestamp,
    };

    this.setBookSide(assetId, tob);
    this.log.debug(`Book snapshot for ${this.sideLabel(assetId)}: bid=${tob.bestBid} ask=${tob.bestAsk}`);
    this.emit("book", { assetId, tob });
  }

  private handlePriceChange(event: Record<string, unknown>): void {
    const timestamp = this.parseTimestamp(event.timestamp);
    const changes = event.price_changes || event.changes;
    if (!Array.isArray(changes)) return;

    for (const change of changes) {
      const assetId = (change as Record<string, unknown>).asset_id as string;
      const rawBestBid = (change as Record<string, unknown>).best_bid;
      const rawBestAsk = (change as Record<string, unknown>).best_ask;

      if (rawBestBid !== undefined && rawBestAsk !== undefined) {
        const bestBid = typeof rawBestBid === "string" ? parseFloat(rawBestBid) : (rawBestBid as number);
        const bestAsk = typeof rawBestAsk === "string" ? parseFloat(rawBestAsk) : (rawBestAsk as number);

        const current = this.getBookForAsset(assetId);
        const tob: TopOfBook = {
          bestBid,
          bestAsk,
          bidSize: current?.bidSize ?? 0,
          askSize: current?.askSize ?? 0,
          timestamp,
        };
        this.setBookSide(assetId, tob);
      }
    }

    this.emit("priceChange", event);
  }

  private parseLevels(raw: unknown, ascending: boolean): OrderLevel[] {
    if (!Array.isArray(raw)) return [];
    return raw.map((lvl: Record<string, unknown>) => ({
      price: typeof lvl.price === "string" ? parseFloat(lvl.price as string) : (lvl.price as number),
      size: typeof lvl.size === "string" ? parseFloat(lvl.size as string) : (lvl.size as number),
    })).sort((a, b) => ascending ? a.price - b.price : b.price - a.price);
  }

  private parseTimestamp(raw: unknown): number {
    if (typeof raw === "number") return raw;
    if (typeof raw === "string") return parseInt(raw, 10);
    return Date.now();
  }

  private setBookSide(assetId: string, tob: TopOfBook): void {
    if (assetId === this.upTokenId) {
      this.bookState.up = tob;
    } else if (assetId === this.downTokenId) {
      this.bookState.down = tob;
    }
  }

  private getBookForAsset(assetId: string): TopOfBook | null {
    if (assetId === this.upTokenId) return this.bookState.up;
    if (assetId === this.downTokenId) return this.bookState.down;
    return null;
  }

  private sideLabel(assetId: string): string {
    if (assetId === this.upTokenId) return "UP";
    if (assetId === this.downTokenId) return "DOWN";
    return "UNKNOWN";
  }

  getBookState(): BookState {
    return this.bookState;
  }
}
