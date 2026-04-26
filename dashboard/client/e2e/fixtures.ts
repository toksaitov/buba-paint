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
      execution_mode: "live_readonly",
      live_session_status: "readonly_ready",
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

  await page.route("**/api/bots/*/trading/summary", async (route) => {
    await json(route, {
      runtime_mode: "live_readonly",
      trading_state: "readonly",
      process_state: "running",
      venue_health: {
        state: "healthy",
        label: "Venue connected",
        detail: "Authenticated venue monitoring and user-stream health are green.",
      },
      account_health: {
        state: "healthy",
        label: "Account tracked",
        detail: "Real cash, allowance, and equity decomposition are available.",
      },
      reconciliation_health: {
        state: "healthy",
        label: "Clean",
        detail: "No critical reconciliation events are currently recorded.",
      },
      shadow_summary: {
        balance: 250.5,
        starting_balance: 200,
        total_pnl: 50.5,
        total_trades: 12,
        wins: 8,
        losses: 4,
        win_rate: 0.667,
        max_drawdown_pct: 0.12,
        high_water_mark: 275,
        uptime_hours: 4.5,
        open_trades: 2,
        current_window: {
          market_id: "mkt-1",
          question: "Will BTC go up?",
          end_time: 1_716_000_300_000,
        },
      },
      real_account_summary: {
        session_id: 1,
        session_status: "readonly_ready",
        session_started_at_ms: 1_716_000_000_000,
        cash_cap_usd: 100,
        available_cash: 99.17,
        reserved_cash: 0,
        inventory_mark_value: 0,
        redeemable_value: 0,
        pending_redeem_value: 0,
        total_equity: 99.17,
        allowance_available: 99.17,
        latest_snapshot_at_ms: 1_716_000_100_000,
        provider: "polymarket",
        user_stream_status: "ok",
        last_user_stream_connected_at_ms: 1_716_000_050_000,
        last_user_stream_event_at_ms: null,
        last_account_refresh_at_ms: 1_716_000_100_000,
        wallet_address: "0x1234",
        proxy_wallet: "0xabcd",
        enabled_strategies: ["latency-arb", "spread-capture"],
        open_orders: 0,
        pending_redemptions: 0,
        critical_reconciliation_events: 0,
        config_fingerprint: "{\"execution_mode\":\"live_readonly\"}",
      },
      capabilities: {
        preflight: { enabled: false, reason: "Dashboard preflight action is not wired in this pass." },
        arm: { enabled: false, reason: "Live trading remains gated in this pass." },
        disarm: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        cancel_all: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        stop_after_flat: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        redeem: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
        kill_switch: { enabled: false, reason: "No dashboard action endpoint exists in this pass." },
      },
      alerts: [],
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

  await page.route("**/api/bots/*/equity/series**", async (route) => {
    await json(route, {
      baseline: { id: 1, timestamp: 0, event: "baseline", balance: 200 },
      points: [
        { id: 2, timestamp: 1_716_000_000_000, event: "init", balance: 200 },
        { id: 3, timestamp: 1_716_000_100_000, event: "settlement", balance: 250.5 },
      ],
    });
  });

  await page.route("**/api/bots/*/signals/groups**", async (route) => {
    await json(route, {
      groups: [
        {
          id: "mkt-1:latency-arb:UP:1716000000000",
          first_timestamp: 1_716_000_000_000,
          last_timestamp: 1_716_000_000_000,
          count: 1,
          strategy: "latency-arb",
          direction: "UP",
          market_id: "mkt-1",
          binance_price: 68000,
          chainlink_price: 68010,
          up_ask: 0.54,
          down_ask: 0.46,
          momentum: 0.0012,
        },
      ],
      raw_rows_scanned: 1,
      quiet_gap_ms: 5000,
    });
  });

  await page.route("**/api/bots/*/signals**", async (route) => {
    if (route.request().url().includes("/signals/groups")) {
      await json(route, {
        groups: [
          {
            id: "mkt-1:latency-arb:UP:1716000000000",
            first_timestamp: 1_716_000_000_000,
            last_timestamp: 1_716_000_000_000,
            count: 1,
            strategy: "latency-arb",
            direction: "UP",
            market_id: "mkt-1",
            binance_price: 68000,
            chainlink_price: 68010,
            up_ask: 0.54,
            down_ask: 0.46,
            momentum: 0.0012,
          },
        ],
        raw_rows_scanned: 1,
        quiet_gap_ms: 5000,
      });
      return;
    }
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

  await page.route("**/api/bots/*/live/status", async (route) => {
    await json(route, {
      latest_session: null,
      latest_account_snapshot: null,
      open_orders: 0,
      pending_redemptions: 0,
      critical_reconciliation_events: 0,
    });
  });

  await page.route("**/api/bots/*/live/sessions**", async (route) => {
    await json(route, {
      sessions: [
        {
          id: 1,
          started_at_ms: 1_716_000_000_000,
          ended_at_ms: null,
          status: "readonly_ready",
          execution_mode: "live_readonly",
          wallet_address: "0x1234",
          proxy_wallet: "0xabcd",
          enabled_strategies_json: "[\"latency-arb\",\"spread-capture\"]",
          config_fingerprint: "{\"execution_mode\":\"live_readonly\"}",
          cash_cap_usd: 100,
          details_json: null,
        },
      ],
    });
  });

  await page.route("**/api/bots/*/live/orders**", async (route) => {
    await json(route, { orders: [] });
  });

  await page.route("**/api/bots/*/live/fills**", async (route) => {
    await json(route, { fills: [] });
  });

  await page.route("**/api/bots/*/live/redemptions**", async (route) => {
    await json(route, { redemptions: [] });
  });

  await page.route("**/api/bots/*/live/reconciliation**", async (route) => {
    await json(route, { events: [] });
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
