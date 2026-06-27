import http from "node:http";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createServer } from "../server.js";
import {
  ProviderStageError,
  StubSidecarProvider,
  type SidecarProvider,
} from "../provider.js";
import { loadConfig } from "../config.js";
import type {
  LiveAccountState,
  LiveActivityResponse,
  LiveCancellationResponse,
  LiveOrderIntentRequest,
  LiveOrderIntentResponse,
  LivePreflightRequest,
  LivePreflightResponse,
  LiveRedemptionResponse,
} from "../types.js";

const servers: http.Server[] = [];

async function startServer(
  provider: SidecarProvider = new StubSidecarProvider(
    loadConfig({
      SIDECAR_PORT: "0",
      POLYMARKET_PROXY_WALLET: "0xproxy",
    }),
  ),
): Promise<{ url: string; close: () => Promise<void> }> {
  const config = loadConfig({
    SIDECAR_PORT: "0",
    POLYMARKET_PROXY_WALLET: "0xproxy",
  });
  const server = createServer(provider, config);
  servers.push(server);
  await new Promise<void>((resolve) => {
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("failed to bind test server");
  }
  return {
    url: `http://127.0.0.1:${address.port}`,
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((error) => {
          if (error) reject(error);
          else resolve();
        });
      }),
  };
}

afterEach(async () => {
  await Promise.all(
    servers.splice(0).map(
      (server) =>
        new Promise<void>((resolve) => {
          server.close(() => resolve());
        }),
    ),
  );
});

describe("sidecar server", () => {
  it("returns health with additive readiness fields", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/health`);
    const json = (await response.json()) as {
      ok: boolean;
      proxy_wallet: string | null;
      ready: boolean;
      readiness_status: string;
      last_account_refresh_error: string | null;
    };
    expect(response.ok).toBe(true);
    expect(json.ok).toBe(true);
    expect(json.proxy_wallet).toBe("0xproxy");
    expect(json.ready).toBe(false);
    expect(json.readiness_status).toBe("failed");
    expect(json.last_account_refresh_error).toBe("stub provider");
  });

  it("runs preflight through the stub provider", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/preflight`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        execution_mode: "live_readonly",
        clob_api_url: "https://clob.polymarket.com",
        gamma_api_url: "https://gamma-api.polymarket.com",
        strategy_readiness: [],
        budget_limits: {
          cash_cap_usd: 100,
          max_single_order_usd: 10,
          max_open_notional_usd: 25,
          max_daily_loss_usd: 15,
          max_session_drawdown_usd: 20,
          min_required_cash_usd: 25,
        },
      }),
    });
    const json = (await response.json()) as {
      ok: boolean;
      geoblock_status: string;
      auth_status: string;
      errors: string[];
    };
    expect(response.ok).toBe(true);
    expect(json.ok).toBe(false);
    expect(json.geoblock_status).toBe("failed");
    expect(json.auth_status).toBe("failed");
    expect(json.errors.length).toBeGreaterThan(0);
  });

  it("runs activity through the stub provider", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/activity`);
    const json = (await response.json()) as LiveActivityResponse;

    expect(response.ok).toBe(true);
    expect(json.user_stream_status).toBe("failed");
    expect(json.user_stream_events).toEqual([]);
    expect(json.clob_trades).toEqual([]);
  });

  it("maps provider stage failures to 503", async () => {
    const provider: SidecarProvider = {
      health: async () => ({
        ok: true,
        ready: true,
        readiness_status: "ready" as const,
        mode: "local-sidecar",
        provider: "test",
        signature_type: 1,
        wallet_address: null,
        proxy_wallet: null,
        auth_configured: true,
        relayer_api_key_present: false,
        user_stream_status: "ok" as const,
        last_user_stream_connected_at_ms: null,
        last_user_stream_event_at_ms: null,
        last_user_stream_error: null,
        last_successful_account_refresh_at_ms: null,
        last_account_refresh_error: null,
        last_user_stream_disconnect_at_ms: null,
        last_user_stream_disconnect_reason: null,
        consecutive_user_stream_failures: 0,
      }),
      accountState: async (): Promise<LiveAccountState> => {
        throw new ProviderStageError("positions", "timed out");
      },
      activity: async (): Promise<LiveActivityResponse> => {
        throw new Error("unexpected");
      },
      preflight: async (_request: LivePreflightRequest): Promise<LivePreflightResponse> => {
        throw new Error("unexpected");
      },
      submitOrderIntent: async (
        _request: LiveOrderIntentRequest,
      ): Promise<LiveOrderIntentResponse> => {
        throw new Error("unexpected");
      },
      cancelOrder: async (_orderId: string): Promise<LiveCancellationResponse> => {
        throw new Error("unexpected");
      },
      cancelAll: async (): Promise<LiveCancellationResponse> => {
        throw new Error("unexpected");
      },
      redeemAll: async (): Promise<LiveRedemptionResponse> => {
        throw new Error("unexpected");
      },
      onchainPosition: async () => {
        throw new Error("unexpected");
      },
      close: async () => undefined,
    };

    const { url } = await startServer(provider);
    const response = await fetch(`${url}/account`);
    const json = (await response.json()) as { error: string };

    expect(response.status).toBe(503);
    expect(json.error).toBe("positions: timed out");
  });

  it("rejects malformed preflight requests", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/preflight`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{\"execution_mode\":",
    });

    const json = (await response.json()) as { error: string };
    expect(response.status).toBe(400);
    expect(json.error).toContain("valid JSON");
  });

  it("rejects malformed order requests", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/orders`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        session_id: 1,
      }),
    });

    const json = (await response.json()) as { error: string };
    expect(response.status).toBe(400);
    expect(json.error).toBe("invalid order intent request");
  });

  it("rejects cancel requests without order ids", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/cancel`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });

    const json = (await response.json()) as { error: string };
    expect(response.status).toBe(400);
    expect(json.error).toBe("order_id is required");
  });

  it("keeps cancel-all and redeem-all explicitly stubbed", async () => {
    const { url } = await startServer();

    const cancelResponse = await fetch(`${url}/cancel-all`, { method: "POST" });
    const cancelJson = (await cancelResponse.json()) as {
      ok: boolean;
      cancelled: number;
      details_json: string | null;
    };
    expect(cancelResponse.ok).toBe(true);
    expect(cancelJson.ok).toBe(false);
    expect(cancelJson.cancelled).toBe(0);
    expect(cancelJson.details_json).toContain("not implemented");

    const redeemResponse = await fetch(`${url}/redeem-all`, { method: "POST" });
    const redeemJson = (await redeemResponse.json()) as {
      ok: boolean;
      submitted: number;
      details_json: string | null;
    };
    expect(redeemResponse.ok).toBe(true);
    expect(redeemJson.ok).toBe(false);
    expect(redeemJson.submitted).toBe(0);
    expect(redeemJson.details_json).toContain("not implemented");
  });

  it("routes write endpoints through the configured provider", async () => {
    let seenAmountUsd: number | null | undefined = null;
    const provider: SidecarProvider = {
      health: async () => ({
        ok: true,
        ready: true,
        readiness_status: "ready" as const,
        mode: "local-sidecar",
        provider: "test",
        signature_type: 1,
        wallet_address: null,
        proxy_wallet: null,
        auth_configured: true,
        relayer_api_key_present: false,
        user_stream_status: "ok" as const,
        last_user_stream_connected_at_ms: null,
        last_user_stream_event_at_ms: null,
        last_user_stream_error: null,
        last_successful_account_refresh_at_ms: null,
        last_account_refresh_error: null,
        last_user_stream_disconnect_at_ms: null,
        last_user_stream_disconnect_reason: null,
        consecutive_user_stream_failures: 0,
      }),
      accountState: async (): Promise<LiveAccountState> => {
        throw new Error("unexpected");
      },
      activity: async (): Promise<LiveActivityResponse> => ({
        timestamp_ms: 1000,
        user_stream_status: "ok",
        user_stream_events: [],
        clob_trades: [],
        details_json: "{\"provider\":\"test\"}",
      }),
      preflight: async (_request: LivePreflightRequest): Promise<LivePreflightResponse> => {
        throw new Error("unexpected");
      },
      submitOrderIntent: async (
        request: LiveOrderIntentRequest,
      ): Promise<LiveOrderIntentResponse> => {
        seenAmountUsd = request.amount_usd;
        return {
          ok: true,
          venue_order_id: "0xvenue",
          client_order_id: request.client_order_id,
          status: "matched",
          status_reason: null,
          accepted_size: 4,
          details_json: "{\"provider\":\"test\"}",
        };
      },
      cancelOrder: async (_orderId: string): Promise<LiveCancellationResponse> => ({
        ok: true,
        cancelled: 1,
        details_json: "{\"provider\":\"test\"}",
      }),
      cancelAll: async (): Promise<LiveCancellationResponse> => ({
        ok: true,
        cancelled: 2,
        details_json: "{\"provider\":\"test\"}",
      }),
      redeemAll: async (): Promise<LiveRedemptionResponse> => ({
        ok: true,
        submitted: 3,
        details_json: "{\"provider\":\"test\"}",
      }),
      onchainPosition: async () => {
        throw new Error("unexpected");
      },
      close: async () => undefined,
    };
    const { url } = await startServer(provider);

    const orderResponse = await fetch(`${url}/orders`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        session_id: 1,
        intent_id: 2,
        market_id: "0xcondition",
        token_id: "up-token",
        side: "BUY",
        order_type: "FOK",
        limit_price: 0.5,
        size: 5,
        amount_usd: 5,
        client_order_id: "client-1",
        details_json: null,
      }),
    });
    const cancelResponse = await fetch(`${url}/cancel`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ order_id: "0xvenue" }),
    });
    const cancelAllResponse = await fetch(`${url}/cancel-all`, { method: "POST" });
    const redeemResponse = await fetch(`${url}/redeem-all`, { method: "POST" });

    expect(orderResponse.ok).toBe(true);
    expect(await orderResponse.json()).toMatchObject({ ok: true });
    expect(seenAmountUsd).toBe(5);
    expect((await cancelResponse.json()) as { cancelled: number }).toMatchObject({
      cancelled: 1,
    });
    expect((await cancelAllResponse.json()) as { cancelled: number }).toMatchObject({
      cancelled: 2,
    });
    expect((await redeemResponse.json()) as { submitted: number }).toMatchObject({
      submitted: 3,
    });
  });

  it("closes the provider exactly once when the server shuts down", async () => {
    const close = vi.fn(async () => undefined);
    const provider: SidecarProvider = {
      health: async () => ({
        ok: true,
        ready: true,
        readiness_status: "ready" as const,
        mode: "local-sidecar",
        provider: "test",
        signature_type: 1,
        wallet_address: null,
        proxy_wallet: null,
        auth_configured: true,
        relayer_api_key_present: false,
        user_stream_status: "ok" as const,
        last_user_stream_connected_at_ms: null,
        last_user_stream_event_at_ms: null,
        last_user_stream_error: null,
        last_successful_account_refresh_at_ms: null,
        last_account_refresh_error: null,
        last_user_stream_disconnect_at_ms: null,
        last_user_stream_disconnect_reason: null,
        consecutive_user_stream_failures: 0,
      }),
      accountState: async (): Promise<LiveAccountState> => {
        throw new Error("unexpected");
      },
      activity: async (): Promise<LiveActivityResponse> => {
        throw new Error("unexpected");
      },
      preflight: async (_request: LivePreflightRequest): Promise<LivePreflightResponse> => {
        throw new Error("unexpected");
      },
      submitOrderIntent: async (
        _request: LiveOrderIntentRequest,
      ): Promise<LiveOrderIntentResponse> => {
        throw new Error("unexpected");
      },
      cancelOrder: async (_orderId: string): Promise<LiveCancellationResponse> => {
        throw new Error("unexpected");
      },
      cancelAll: async (): Promise<LiveCancellationResponse> => {
        throw new Error("unexpected");
      },
      redeemAll: async (): Promise<LiveRedemptionResponse> => {
        throw new Error("unexpected");
      },
      onchainPosition: async () => {
        throw new Error("unexpected");
      },
      close,
    };

    const { close: stop } = await startServer(provider);
    await stop();
    await Promise.resolve();

    expect(close).toHaveBeenCalledTimes(1);
  });
});
