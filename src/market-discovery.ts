import { EventEmitter } from "node:events";
import { CONFIG } from "./config.js";
import { createLogger } from "./utils/logger.js";
import type { MarketWindow } from "./types.js";

const log = createLogger("discovery");

const WINDOW_DURATION_S = 300; // 5 minutes in seconds

export class MarketDiscovery extends EventEmitter {
  private currentWindow: MarketWindow | null = null;
  private pollTimer: ReturnType<typeof setInterval> | null = null;
  private windowCloseTimer: ReturnType<typeof setTimeout> | null = null;

  async start(): Promise<void> {
    log.info("Starting market discovery");
    await this.poll();
    this.pollTimer = setInterval(() => this.poll(), CONFIG.GAMMA_POLL_INTERVAL);
  }

  stop(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
    if (this.windowCloseTimer) {
      clearTimeout(this.windowCloseTimer);
      this.windowCloseTimer = null;
    }
    log.info("Market discovery stopped");
  }

  getCurrentWindow(): MarketWindow | null {
    return this.currentWindow;
  }

  private async poll(): Promise<void> {
    try {
      // Check if current window has expired
      if (this.currentWindow && Date.now() >= this.currentWindow.endTime) {
        this.closeCurrentWindow();
      }

      const window = await this.findCurrentWindow();
      if (!window) {
        log.debug("No active 5-min BTC market found");
        return;
      }

      // Already tracking this window
      if (this.currentWindow?.marketId === window.marketId) return;

      // Close previous window if still open
      if (this.currentWindow) {
        this.closeCurrentWindow();
      }

      log.info(`New market window: ${window.question}`);
      log.info(`  UP token:  ${window.upTokenId.slice(0, 30)}...`);
      log.info(`  DOWN token: ${window.downTokenId.slice(0, 30)}...`);
      log.info(`  Ends at: ${new Date(window.endTime).toISOString()}`);

      this.currentWindow = window;
      this.emit("newWindow", window);
      this.scheduleWindowClose(window);
    } catch (err) {
      log.error("Poll error", err);
    }
  }

  private async findCurrentWindow(): Promise<MarketWindow | null> {
    // BTC 5-min markets use a predictable event slug: btc-updown-5m-{unix_ts}
    // where unix_ts is the window start time rounded down to 300 seconds
    const nowS = Math.floor(Date.now() / 1000);
    const currentWindowStart = Math.floor(nowS / WINDOW_DURATION_S) * WINDOW_DURATION_S;

    // Try current window first, then next window
    const candidates = [currentWindowStart, currentWindowStart + WINDOW_DURATION_S];

    for (const startTs of candidates) {
      const slug = `btc-updown-5m-${startTs}`;
      const window = await this.fetchEventBySlug(slug, startTs);
      if (window) {
        return window;
      }
    }

    return null;
  }

  private async fetchEventBySlug(slug: string, startTs: number): Promise<MarketWindow | null> {
    const url = `${CONFIG.GAMMA_API_URL}/events/slug/${slug}`;
    log.debug(`Fetching ${url}`);

    try {
      const resp = await fetch(url);
      if (!resp.ok) {
        if (resp.status !== 404) {
          log.warn(`Gamma API error: ${resp.status} for ${slug}`);
        }
        return null;
      }

      const event = await resp.json() as Record<string, unknown>;
      const markets = event.markets as Array<Record<string, unknown>> | undefined;
      if (!markets || markets.length === 0) return null;

      const m = markets[0];

      // Skip closed markets
      if (m.closed) return null;

      const outcomes = typeof m.outcomes === "string"
        ? JSON.parse(m.outcomes as string) as string[]
        : (m.outcomes as string[]) ?? [];
      const clobTokenIds = typeof m.clobTokenIds === "string"
        ? JSON.parse(m.clobTokenIds as string) as string[]
        : (m.clobTokenIds as string[]) ?? [];

      if (outcomes.length !== 2 || clobTokenIds.length !== 2) return null;

      const upIdx = outcomes.findIndex((o) => o.toLowerCase() === "up");
      const downIdx = outcomes.findIndex((o) => o.toLowerCase() === "down");
      if (upIdx === -1 || downIdx === -1) return null;

      const endTime = new Date(m.endDate as string).getTime();
      if (endTime <= Date.now()) return null;

      return {
        marketId: String(m.id),
        question: String(m.question),
        upTokenId: clobTokenIds[upIdx],
        downTokenId: clobTokenIds[downIdx],
        conditionId: String(m.conditionId),
        startTime: startTs * 1000,
        endTime,
        slug,
      };
    } catch (err) {
      log.error(`Error fetching event ${slug}`, err);
      return null;
    }
  }

  private closeCurrentWindow(): void {
    if (!this.currentWindow) return;
    log.info(`Window closed: ${this.currentWindow.question}`);
    this.emit("windowClosed", this.currentWindow);
    this.currentWindow = null;
    if (this.windowCloseTimer) {
      clearTimeout(this.windowCloseTimer);
      this.windowCloseTimer = null;
    }
  }

  private scheduleWindowClose(window: MarketWindow): void {
    if (this.windowCloseTimer) {
      clearTimeout(this.windowCloseTimer);
    }
    const delay = Math.max(0, window.endTime - Date.now());
    this.windowCloseTimer = setTimeout(() => {
      this.closeCurrentWindow();
      // Immediately poll for the next window
      this.poll();
    }, delay);
    log.info(`Window close scheduled in ${Math.round(delay / 1000)}s`);
  }
}
