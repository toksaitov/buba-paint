import type { Page, Route } from "@playwright/test";

function json(route: Route, body: unknown, status = 200) {
  return route.fulfill({
    status,
    contentType: "application/json",
    body: JSON.stringify(body),
  });
}

export async function installMockWebSocket(page: Page) {
  await page.addInitScript(() => {
    class MockWebSocket {
      static CONNECTING = 0;
      static OPEN = 1;
      static CLOSING = 2;
      static CLOSED = 3;

      readyState = MockWebSocket.CONNECTING;
      url: string;
      onopen: ((event: Event) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;

      constructor(url: string) {
        this.url = url;
        queueMicrotask(() => {
          this.readyState = MockWebSocket.OPEN;
          this.onopen?.(new Event("open"));
        });
      }

      send() {}

      close() {
        this.readyState = MockWebSocket.CLOSED;
        this.onclose?.(new CloseEvent("close"));
      }

      addEventListener(type: string, listener: EventListener) {
        if (type === "open") this.onopen = listener as (event: Event) => void;
        if (type === "close") this.onclose = listener as (event: CloseEvent) => void;
        if (type === "message") this.onmessage = listener as (event: MessageEvent) => void;
        if (type === "error") this.onerror = listener as (event: Event) => void;
      }

      removeEventListener() {}
    }

    Object.defineProperty(window, "WebSocket", {
      value: MockWebSocket,
      writable: true,
    });
  });
}

export async function stubApi(page: Page) {
  await page.route("**/api/auth/login", async (route) => {
    await json(route, {
      token: "jwt-token",
      user: { id: "user-1", username: "admin", role: "admin" },
    });
  });

  await page.route("**/api/auth/me", async (route) => {
    await json(route, { id: "user-1", username: "admin", role: "admin" });
  });

  await page.route("**/api/bots", async (route) => {
    await json(route, {
      bots: [
        { id: "paint", name: "Paint" },
        { id: "paper-2", name: "Paper Two" },
      ],
    });
  });

  await page.route("**/api/bots/*/status", async (route) => {
    await json(route, {
      balance: 250.5,
      starting_balance: 200,
      total_trades: 12,
      wins: 8,
      losses: 4,
      win_rate: 66.7,
      total_pnl: 50.5,
      max_drawdown_pct: 0.12,
      high_water_mark: 275.0,
      uptime_hours: 4.5,
      open_trades: 2,
      last_tick_at: 1_716_000_000_000,
      current_window: {
        market_id: "mkt-1",
        question: "Will BTC go up?",
        end_time: 1_716_000_300_000,
      },
    });
  });

  await page.route("**/api/bots/*/trades**", async (route) => {
    await json(route, {
      trades: [
        {
          id: 1,
          timestamp: 1_716_000_000_000,
          market_id: "mkt-1",
          strategy: "latency-arb",
          side: "UP",
          token_id: "tok-up",
          entry_price: 0.54,
          size: 25,
          status: "closed",
          pnl: 12.5,
          settlement_price: 1,
          resolved_at: 1_716_000_100_000,
          fill_status: "filled",
          execution_fidelity: "legacy_snapshot",
          filled_size: 25,
          avg_fill_price: 0.54,
        },
      ],
      total: 1,
      page: 1,
      per_page: 50,
    });
  });

  await page.route("**/api/bots/*/balance**", async (route) => {
    await json(route, {
      entries: [
        { id: 1, timestamp: 1_716_000_000_000, event: "init", balance: 200 },
        { id: 2, timestamp: 1_716_000_100_000, event: "settlement", balance: 250.5 },
      ],
    });
  });

  await page.route("**/api/bots/*/signals**", async (route) => {
    await json(route, {
      signals: [
        {
          id: 10,
          timestamp: 1_716_000_000_000,
          strategy: "latency-arb",
          direction: "UP",
          binance_price: 68000,
          chainlink_price: 68010,
          up_ask: 0.54,
          down_ask: 0.46,
          metadata: "{\"momentum\":0.0012}",
          market_id: "mkt-1",
          execution_fidelity: "legacy_snapshot",
        },
      ],
    });
  });

  await page.route("**/api/bots/*/logs**", async (route) => {
    await json(route, {
      lines: ["booted paint", "connected to feeds", "latency arb signal"],
    });
  });

  await page.route("**/api/bots/*/stats", async (route) => {
    await json(route, {
      by_strategy: {
        "latency-arb": {
          trades: 12,
          wins: 8,
          losses: 4,
          win_rate: 66.7,
          total_pnl: 50.5,
        },
      },
    });
  });

  await page.route("**/api/bots/*/process", async (route) => {
    await json(route, { active: true, pid: 1234, uptime_secs: 3600 });
  });

  await page.route("**/api/bots/*/start", async (route) => {
    await json(route, { active: true, pid: 1234, uptime_secs: 1 });
  });

  await page.route("**/api/bots/*/stop", async (route) => {
    await json(route, { active: false, pid: null, uptime_secs: null });
  });

  await page.route("**/api/bots/*/restart", async (route) => {
    await json(route, { active: true, pid: 2345, uptime_secs: 0 });
  });
}
