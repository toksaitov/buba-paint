import { describe, expect, it } from "vitest";
import {
  PolymarketReadonlyProvider,
  StubSidecarProvider,
} from "../provider.js";
import { loadConfig } from "../config.js";
import type { LivePreflightRequest } from "../types.js";

describe("StubSidecarProvider", () => {
  it("keeps preflight in contract-only mode even with credentials", async () => {
    const provider = new StubSidecarProvider(
      loadConfig({
        POLYMARKET_PRIVATE_KEY: "pk",
        POLYMARKET_PROXY_WALLET: "0xproxy",
        POLYMARKET_FUNDER: "0xfunder",
        POLYMARKET_RELAYER_API_KEY: "relayer",
      }),
    );

    const response = await provider.preflight({
      execution_mode: "live_readonly",
      clob_api_url: "https://clob.polymarket.com",
      gamma_api_url: "https://gamma-api.polymarket.com",
      strategy_readiness: [],
      budget_limits: {
        cash_cap_usd: 90,
        max_single_order_usd: 7.5,
        max_open_notional_usd: 20,
        max_daily_loss_usd: 10,
        max_session_drawdown_usd: 12,
        min_required_cash_usd: 25,
      },
    });

    expect(response.ok).toBe(false);
    expect(response.wallet_address).toBe("0xfunder");
    expect(response.proxy_wallet).toBe("0xproxy");
    expect(response.auth_status).toBe("ok");
    expect(response.geoblock_status).toBe("failed");
    expect(response.allowance_status).toBe("failed");
    expect(response.user_stream_status).toBe("failed");
    expect(response.available_cash_usd).toBeNull();
    expect(response.errors[0]).toContain("Stub sidecar");
  });
});

describe("PolymarketReadonlyProvider", () => {
  const nowMs = 1_700_000_000_000;
  const request: LivePreflightRequest = {
    execution_mode: "live_readonly",
    clob_api_url: "https://clob.polymarket.com",
    gamma_api_url: "https://gamma-api.polymarket.com",
    strategy_readiness: [],
    budget_limits: {
      cash_cap_usd: 90,
      max_single_order_usd: 7.5,
      max_open_notional_usd: 20,
      max_daily_loss_usd: 10,
        max_session_drawdown_usd: 12,
        min_required_cash_usd: 25,
      },
    };

  function createProvider(options?: {
    ensureConnectedError?: string;
    openOrders?: Array<Record<string, string>>;
    positions?: Array<Record<string, number | string | boolean>>;
    createApiKeyError?: string;
    balance?: string;
    allowance?: string | null;
  }) {
    const config = loadConfig({
      POLYMARKET_PRIVATE_KEY:
        "0x59c6995e998f97a5a0044966f0945382db3e5e8a0a5729b6b6b6f8c0d4b47a6a",
      POLYMARKET_PROXY_WALLET: "0xproxy",
      POLYMARKET_FUNDER: "0xfunder",
    });
    let connectedMarkets: string[] = [];

    const provider = new PolymarketReadonlyProvider(config, {
      nowMs: () => nowMs,
      fetchImpl: async (input: URL | RequestInfo) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url === "https://polymarket.com/api/geoblock") {
          return new Response(
            JSON.stringify({ blocked: false, country: "IE", ip: "1.2.3.4" }),
            { status: 200 },
          );
        }
        if (url.includes("/events/slug/btc-updown-5m-")) {
          return new Response(
            JSON.stringify({
              slug: "btc-updown-5m-1700000100",
              markets: [
                {
                  id: "0xcondition",
                  conditionId: "0xcondition",
                  orderMinSize: 5,
                  orderPriceMinTickSize: 0.01,
                  acceptingOrders: true,
                },
              ],
            }),
            { status: 200 },
          );
        }
        if (url.startsWith("https://data-api.polymarket.com/positions")) {
          return new Response(JSON.stringify(options?.positions ?? []), { status: 200 });
        }
        throw new Error(`unexpected fetch ${url}`);
      },
      createClobClient: () => ({
        createApiKey: async () => {
          if (options?.createApiKeyError) {
            throw new Error(options.createApiKeyError);
          }
          return {
            key: "key",
            secret: "secret",
            passphrase: "passphrase",
          };
        },
        deriveApiKey: async () => ({
          key: "derived-key",
          secret: "derived-secret",
          passphrase: "derived-passphrase",
        }),
        getServerTime: async () => Math.floor(nowMs / 1000),
        getBalanceAllowance: async () => {
          const allowances =
            options?.allowance === null
              ? undefined
              : ({
                  "0xexchange": options?.allowance ?? "80000000",
                } satisfies Record<string, string>);
          return {
            balance: options?.balance ?? "100000000",
            allowances,
          };
        },
        getOpenOrders: async () =>
          (options?.openOrders as never as []) ??
          [
            {
              id: "0xorder",
              status: "OPEN",
              owner: "owner",
              maker_address: "maker",
              market: "0xcondition",
              asset_id: "0xasset",
              side: "BUY",
              original_size: "10",
              size_matched: "0",
              price: "0.5",
              associate_trades: [],
              outcome: "YES",
              created_at: 1,
              expiration: "2",
              order_type: "FOK",
            },
          ],
      }),
      createUserStreamMonitor: () => ({
        ensureConnected: async (_auth, markets) => {
          connectedMarkets = markets;
          if (options?.ensureConnectedError) {
            throw new Error(options.ensureConnectedError);
          }
        },
        snapshot: () => ({
          status: options?.ensureConnectedError ? "failed" : "ok",
          lastConnectedAtMs: options?.ensureConnectedError ? null : nowMs,
          lastEventAtMs: null,
          lastError: options?.ensureConnectedError ?? null,
          subscribedMarkets: connectedMarkets,
        }),
        close: () => undefined,
      }),
    });

    return provider;
  }

  it("runs a successful readonly preflight against the real provider contract", async () => {
    const provider = createProvider();
    const response = await provider.preflight(request);

    expect(response.ok).toBe(true);
    expect(response.wallet_address).toBe("0xfunder");
    expect(response.proxy_wallet).toBe("0xproxy");
    expect(response.geoblock_status).toBe("ok");
    expect(response.auth_status).toBe("ok");
    expect(response.clock_status).toBe("ok");
    expect(response.allowance_status).toBe("ok");
    expect(response.user_stream_status).toBe("ok");
    expect(response.available_cash_usd).toBe(95);
    expect(response.legal_order_min_usd).toBe(5);
    expect(response.details_json).toContain("\"provider\":\"polymarket\"");
  });

  it("falls back to derive-api-key when create-api-key is unavailable", async () => {
    const provider = createProvider({ createApiKeyError: "Could not create api key" });
    const response = await provider.preflight(request);

    expect(response.ok).toBe(true);
    expect(response.auth_status).toBe("ok");
    expect(response.user_stream_status).toBe("ok");
  });

  it("surfaces authenticated user-stream failures without enabling trading", async () => {
    const provider = createProvider({ ensureConnectedError: "auth rejected" });
    const response = await provider.preflight(request);

    expect(response.ok).toBe(false);
    expect(response.user_stream_status).toBe("failed");
    expect(response.errors.join(" ")).toContain("Authenticated user stream failed");
  });

  it("treats zero allowance as readable venue state, not missing allowance metadata", async () => {
    const provider = createProvider({ balance: "0", allowance: "0" });
    const response = await provider.preflight({
      ...request,
      budget_limits: {
        ...request.budget_limits,
        min_required_cash_usd: 0,
        max_single_order_usd: 10,
      },
    });

    expect(response.allowance_status).toBe("ok");
    expect(response.available_cash_usd).toBe(0);
    expect(response.errors.join(" ")).toContain("legal order minimum");
  });

  it("caps the diagnostic allowance detail when approval is effectively unlimited", async () => {
    const provider = createProvider({
      allowance:
        "115792089237316195423570985008687907853269984665640564039457584007913129639935",
    });
    await provider.preflight(request);
    const account = await provider.accountState();
    const details = JSON.parse(account.details_json ?? "{}") as {
      observed_allowance_usd: number | null;
      observed_allowance_approval_usd: number | null;
    };

    expect(account.allowance_available).toBe(95);
    expect(details.observed_allowance_usd).toBe(100);
    expect(details.observed_allowance_approval_usd).toBeGreaterThan(1e60);
  });

  it("returns a real account decomposition while keeping order flow disabled", async () => {
    const provider = createProvider({
      positions: [
        { currentValue: 12, redeemable: false },
        { currentValue: 7, redeemable: true },
      ],
    });
    await provider.preflight(request);
    const account = await provider.accountState();
    const order = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "mkt-1",
      token_id: "tok-1",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.51,
      size: 5,
      client_order_id: "client-1",
      details_json: null,
    });

    expect(account.cash_available).toBe(95);
    expect(account.cash_reserved_for_orders).toBe(5);
    expect(account.inventory_mark_value).toBe(12);
    expect(account.redeemable_value).toBe(7);
    expect(account.total_equity).toBe(119);
    expect(account.allowance_available).toBe(75);
    expect(account.details_json).toContain("\"open_order_count\":1");
    expect(order.status).toBe("not_implemented");
  });
});
