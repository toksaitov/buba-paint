import { EventEmitter } from "node:events";
import WebSocket from "ws";
import { CONFIG } from "../config.js";
import { createLogger } from "../utils/logger.js";
import type { FeedStatus } from "../types.js";

export abstract class BaseFeed extends EventEmitter {
  protected ws: WebSocket | null = null;
  protected status: FeedStatus = "disconnected";
  protected readonly log;
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private pingTimer: ReturnType<typeof setInterval> | null = null;

  constructor(
    protected readonly name: string,
    protected readonly url: string,
    protected readonly pingIntervalMs: number | null,
  ) {
    super();
    this.log = createLogger(name);
  }

  connect(): void {
    if (this.status === "connecting" || this.status === "connected") return;

    this.status = "connecting";
    this.log.info(`Connecting to ${this.url}`);

    this.ws = new WebSocket(this.url);

    this.ws.on("open", () => {
      this.status = "connected";
      this.reconnectAttempt = 0;
      this.log.info("Connected");
      this.startPing();
      this.onConnected();
      this.emit("connected");
    });

    this.ws.on("message", (data: WebSocket.Data) => {
      try {
        this.onMessage(data.toString());
      } catch (err) {
        this.log.error("Message handling error", err);
      }
    });

    this.ws.on("close", (code: number, reason: Buffer) => {
      this.log.warn(`Disconnected: code=${code} reason=${reason.toString()}`);
      this.cleanup();
      this.status = "disconnected";
      this.emit("disconnected");
      this.scheduleReconnect();
    });

    this.ws.on("error", (err: Error) => {
      this.log.error("WebSocket error", err.message);
    });
  }

  disconnect(): void {
    this.clearReconnect();
    this.cleanup();
    if (this.ws) {
      const ws = this.ws;
      this.ws = null;
      // Attach a no-op error handler BEFORE closing to prevent unhandled
      // 'error' events (e.g. closing a CONNECTING socket throws).
      ws.removeAllListeners();
      ws.on("error", () => {});
      try {
        if (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING) {
          ws.close(1000, "shutdown");
        }
      } catch {
        // Ignore — socket is already closing or closed
      }
    }
    this.status = "disconnected";
    this.log.info("Disconnected (manual)");
  }

  getStatus(): FeedStatus {
    return this.status;
  }

  protected abstract onMessage(data: string): void;

  protected onConnected(): void {
    // Override in subclasses to send subscription messages
  }

  protected send(data: unknown): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }

  private scheduleReconnect(): void {
    this.clearReconnect();
    const delay = Math.min(
      CONFIG.RECONNECT_BASE_DELAY * Math.pow(2, this.reconnectAttempt),
      CONFIG.RECONNECT_MAX_DELAY,
    ) + Math.random() * 1000;
    this.reconnectAttempt++;
    this.log.info(`Reconnecting in ${Math.round(delay)}ms (attempt ${this.reconnectAttempt})`);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }

  private clearReconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private startPing(): void {
    this.stopPing();
    if (!this.pingIntervalMs) return;
    this.pingTimer = setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.ping();
      }
    }, this.pingIntervalMs);
  }

  private stopPing(): void {
    if (this.pingTimer) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
  }

  private cleanup(): void {
    this.stopPing();
  }
}
