import http from "node:http";
import { afterEach, describe, expect, it } from "vitest";
import { createServer } from "../server.js";
import { StubSidecarProvider } from "../provider.js";
import { loadConfig } from "../config.js";

const servers: http.Server[] = [];

async function startServer(): Promise<{ url: string; close: () => Promise<void> }> {
  const config = loadConfig({
    SIDECAR_PORT: "0",
    POLYMARKET_PROXY_WALLET: "0xproxy",
  });
  const server = createServer(new StubSidecarProvider(config), config);
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
  it("returns health", async () => {
    const { url } = await startServer();
    const response = await fetch(`${url}/health`);
    const json = (await response.json()) as { ok: boolean; proxy_wallet: string | null };
    expect(response.ok).toBe(true);
    expect(json.ok).toBe(true);
    expect(json.proxy_wallet).toBe("0xproxy");
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
});
