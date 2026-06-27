import { afterEach, describe, expect, it, vi } from "vitest";
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
    expect(response.signature_type).toBe(1);
    expect(response.geoblock_ip).toBeNull();
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

  afterEach(() => {
    vi.useRealTimers();
  });

  function createProvider(options?: {
    ensureConnectedError?: { value: string | null };
    openOrders?: Array<Record<string, string>>;
    positions?: Array<Record<string, number | string | boolean>>;
    authBootstrapError?: { value: string | null };
    authBootstrapCalls?: { value: number };
    balance?: string;
    allowance?: string | null;
    openOrdersError?: { value: string | null };
    clobMarketInfoError?: { value: unknown | null };
    discoveryMode?: "ok" | "partial_failure";
    httpTimeoutOnPositions?: boolean;
    observedUserAgents?: string[];
    postedOrders?: Array<Record<string, unknown>>;
    orderResponse?: Record<string, unknown>;
    orderError?: { value: unknown | null };
    cancelResponse?: Record<string, unknown>;
    cancelAllResponse?: Record<string, unknown>;
    cancelError?: { value: unknown | null };
    cancelAllError?: { value: unknown | null };
    trades?: Array<Record<string, unknown>>;
    tradesError?: { value: unknown | null };
    relayerExecuteCalls?: Array<Record<string, unknown>>;
    relayerTerminalState?: string;
    builderCredentials?: boolean;
    expectedTakerFeeRate?: string;
  }) {
    const config = loadConfig({
      POLYMARKET_PRIVATE_KEY:
        "0x59c6995e998f97a5a0044966f0945382db3e5e8a0a5729b6b6b6f8c0d4b47a6a",
      POLYMARKET_PROXY_WALLET: "0xproxy",
      POLYMARKET_FUNDER: "0xfunder",
      POLYMARKET_HTTP_TIMEOUT_MS: options?.httpTimeoutOnPositions ? "10" : "5000",
      POLYMARKET_SDK_TIMEOUT_MS: "50",
      POLYMARKET_BUILDER_API_KEY: options?.builderCredentials
        ? "builder-key"
        : undefined,
      POLYMARKET_BUILDER_SECRET: options?.builderCredentials
        ? "builder-secret"
        : undefined,
      POLYMARKET_BUILDER_PASSPHRASE: options?.builderCredentials
        ? "builder-passphrase"
        : undefined,
      POLYMARKET_EXPECTED_TAKER_FEE_RATE: options?.expectedTakerFeeRate,
    });
    let connectedMarkets: string[] = [];

    const provider = new PolymarketReadonlyProvider(config, {
      nowMs: () => nowMs,
      fetchImpl: async (input, init) => {
        const url = typeof input === "string" ? input : input.toString();
        options?.observedUserAgents?.push(
          new Headers(init?.headers).get("user-agent") ?? "",
        );
        if (url === "https://polymarket.com/api/geoblock") {
          return new Response(
            JSON.stringify({ blocked: false, country: "IE", ip: "1.2.3.4" }),
            { status: 200 },
          );
        }
        if (url.includes("/events/slug/btc-updown-5m-")) {
          const isFutureSlot = url.endsWith("-1700000100");
          if (options?.discoveryMode === "partial_failure" && !isFutureSlot) {
            return new Response(JSON.stringify({ error: "boom" }), { status: 500 });
          }
          return new Response(
            JSON.stringify({
              slug: isFutureSlot
                ? "btc-updown-5m-1700000100"
                : "btc-updown-5m-1700000000",
              markets: [
                {
                  id: isFutureSlot ? "0xnext" : "0xcondition",
                  conditionId: isFutureSlot ? "0xnext" : "0xcondition",
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
          if (options?.httpTimeoutOnPositions) {
            return new Promise<Response>(() => undefined);
          }
          return new Response(JSON.stringify(options?.positions ?? []), { status: 200 });
        }
        throw new Error(`unexpected fetch ${url}`);
      },
      createClobClient: () => ({
        createOrDeriveApiKey: async () => {
          if (options?.authBootstrapCalls) {
            options.authBootstrapCalls.value += 1;
          }
          if (options?.authBootstrapError?.value) {
            throw new Error(options.authBootstrapError.value);
          }
          return {
            key: "key",
            secret: "secret",
            passphrase: "passphrase",
          };
        },
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
        getOpenOrders: async () => {
          if (options?.openOrdersError?.value) {
            throw new Error(options.openOrdersError.value);
          }
          return (
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
            ]
          );
        },
        getClobMarketInfo: async (_conditionId: string) => {
          if (options?.clobMarketInfoError?.value) {
            throw options.clobMarketInfoError.value;
          }
          return {
            c: "0xcondition",
            t: [
              { t: "up-token", o: "Up" },
              { t: "down-token", o: "Down" },
            ],
            mts: 0.01,
            nr: false,
            fd: { r: 0.072, e: 1, to: true },
            mbf: 0,
            tbf: 0,
            r: null,
          };
        },
        getTrades: async () => {
          if (options?.tradesError?.value) {
            throw options.tradesError.value;
          }
          return (
            (options?.trades as never as []) ??
            [
              {
                id: "0xtrade",
                taker_order_id: "0xvenue-order",
                market: "0xcondition",
                asset_id: "up-token",
                side: "BUY",
                size: "10",
                fee_rate_bps: "0",
                price: "0.5",
                status: "confirmed",
                match_time: "1700000000",
                last_update: "1700000001",
                outcome: "Up",
                bucket_index: 0,
                owner: "owner",
                maker_address: "maker",
                maker_orders: [],
                transaction_hash: "0xtx",
                trader_side: "TAKER",
              },
            ]
          );
        },
        createAndPostMarketOrder: async (order, marketOptions, orderType) => {
          options?.postedOrders?.push({ order, marketOptions, orderType });
          if (options?.orderError?.value) {
            throw options.orderError.value;
          }
          return {
            success: true,
            errorMsg: "",
            orderID: "0xvenue-order",
            transactionsHashes: [],
            status: "matched",
            takingAmount: "10000000",
            makingAmount: "5000000",
            ...(options?.orderResponse ?? {}),
          };
        },
        cancelOrder: async (_payload) => {
          if (options?.cancelError?.value) {
            throw options.cancelError.value;
          }
          return options?.cancelResponse ?? { canceled: ["0xorder"], not_canceled: {} };
        },
        cancelAll: async () => {
          if (options?.cancelAllError?.value) {
            throw options.cancelAllError.value;
          }
          return (
            options?.cancelAllResponse ?? {
              canceled: ["0xorder-1", "0xorder-2"],
              not_canceled: {},
            }
          );
        },
      }),
      createRelayerClient: () => ({
        execute: async (txns, metadata) => {
          options?.relayerExecuteCalls?.push({ txns, metadata });
          return {
            transactionID: "tx-1",
            state: "STATE_NEW",
            hash: "",
            transactionHash: "",
            getTransaction: async () => [],
            wait: async () => undefined,
          };
        },
        pollUntilState: async () => ({
          transactionID: "tx-1",
          transactionHash: "0xhash",
          from: "0xfunder",
          to: "0xctf",
          proxyAddress: "0xproxy",
          data: "0x",
          nonce: "1",
          value: "0",
          state: options?.relayerTerminalState ?? "STATE_CONFIRMED",
          type: "PROXY",
          metadata: "buba-paint redeem positions",
          createdAt: new Date(nowMs),
          updatedAt: new Date(nowMs),
        }),
      }),
      createUserStreamMonitor: () => ({
        ensureConnected: async (_auth, markets) => {
          connectedMarkets = markets;
          if (options?.ensureConnectedError?.value) {
            throw new Error(options.ensureConnectedError.value);
          }
        },
        snapshot: () => ({
          status: options?.ensureConnectedError?.value ? "failed" : "ok",
          lifecycle: options?.ensureConnectedError?.value ? "reconnecting" : "connected",
          lastConnectedAtMs: options?.ensureConnectedError?.value ? null : nowMs,
          lastEventAtMs: null,
          lastError: options?.ensureConnectedError?.value ?? null,
          lastDisconnectedAtMs: options?.ensureConnectedError?.value ? nowMs : null,
          lastDisconnectReason: options?.ensureConnectedError?.value ?? null,
          consecutiveFailures: options?.ensureConnectedError?.value ? 2 : 0,
          subscribedMarkets: connectedMarkets,
          recentEvents: [],
        }),
        close: () => undefined,
      }),
    });

    return provider;
  }

  it("runs a successful readonly preflight against the real provider contract", async () => {
    const provider = createProvider();
    const response = await provider.preflight(request);
    const health = await provider.health();

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
    expect(response.details_json).toContain("\"clob_contract_version\":\"v2\"");
    expect(response.details_json).toContain("\"collateral_token\":\"pUSD\"");
    expect(response.details_json).toContain("\"metadataSource\":\"clob_v2\"");
    expect(response.details_json).toContain("\"tokenId\":\"up-token\"");
    expect(response.details_json).toContain("\"fee_rate_mismatches\":[]");
    expect(response.details_json).not.toContain("secret");
    expect(response.details_json).not.toContain("passphrase");
    expect(health.ready).toBe(true);
    expect(health.readiness_status).toBe("ready");
    expect(health.last_successful_account_refresh_at_ms).toBe(nowMs);
    expect(health.last_account_refresh_error).toBeNull();
  });

  it("flags a live taker-fee mismatch in preflight without blocking arming", async () => {
    const provider = createProvider({ expectedTakerFeeRate: "0.07" });
    const response = await provider.preflight(request);

    expect(response.ok).toBe(true);
    const details = JSON.parse(response.details_json ?? "{}") as {
      expected_taker_fee_rate: number;
      fee_rate_mismatches: Array<{
        conditionId: string;
        observedRate: number;
        expectedRate: number;
      }>;
    };
    expect(details.expected_taker_fee_rate).toBe(0.07);
    expect(details.fee_rate_mismatches.length).toBeGreaterThan(0);
    expect(details.fee_rate_mismatches[0].observedRate).toBe(0.072);
    expect(details.fee_rate_mismatches[0].expectedRate).toBe(0.07);
  });

  it("sends a stable user agent on public Polymarket HTTP checks", async () => {
    const observedUserAgents: string[] = [];
    const provider = createProvider({ observedUserAgents });

    await provider.preflight(request);
    await provider.accountState();

    expect(observedUserAgents).not.toHaveLength(0);
    expect(
      observedUserAgents.every(
        (value) => value === "buba-polymarket-sidecar/0.1.0",
      ),
    ).toBe(true);
  });

  it("does not permanently cache a rejected auth bootstrap", async () => {
    const calls = { value: 0 };
    const authBootstrapError: { value: string | null } = { value: "boom" };
    const provider = createProvider({ authBootstrapCalls: calls, authBootstrapError });

    const first = await provider.preflight(request);
    expect(first.ok).toBe(false);
    expect(first.errors.join(" ")).toContain("auth_bootstrap: boom");

    authBootstrapError.value = null;
    const second = await provider.preflight(request);

    expect(second.ok).toBe(true);
    expect(calls.value).toBe(2);
  });

  it("invalidates cached auth state on downstream auth-like failures", async () => {
    const calls = { value: 0 };
    const openOrdersError = { value: null as string | null };
    const provider = createProvider({ authBootstrapCalls: calls, openOrdersError });

    await provider.accountState();
    expect(calls.value).toBe(1);

    openOrdersError.value = "401 api key expired";
    await expect(provider.accountState()).rejects.toThrow(
      "open_orders: 401 api key expired",
    );

    openOrdersError.value = null;
    await provider.accountState();
    expect(calls.value).toBe(2);
  });

  it("uses the CLOB V2 create-or-derive API-key bootstrap", async () => {
    const calls = { value: 0 };
    const provider = createProvider({ authBootstrapCalls: calls });
    const response = await provider.preflight(request);

    expect(response.ok).toBe(true);
    expect(response.auth_status).toBe("ok");
    expect(response.user_stream_status).toBe("ok");
    expect(calls.value).toBe(1);
  });

  it("surfaces authenticated user-stream failures without enabling trading", async () => {
    const ensureConnectedError: { value: string | null } = {
      value: "auth rejected",
    };
    const provider = createProvider({ ensureConnectedError });
    const response = await provider.preflight(request);
    const health = await provider.health();

    expect(response.ok).toBe(false);
    expect(response.user_stream_status).toBe("failed");
    expect(response.errors.join(" ")).toContain("Authenticated user stream failed");
    expect(health.readiness_status).toBe("degraded");
    expect(health.consecutive_user_stream_failures).toBe(2);
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

  it("returns a real account decomposition and submits immediate CLOB V2 orders", async () => {
    const postedOrders: Array<Record<string, unknown>> = [];
    const provider = createProvider({
      postedOrders,
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
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.51,
      size: 5,
      amount_usd: 5,
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
    expect(account.details_json).toContain("\"collateral_token\":\"pUSD\"");
    expect(account.details_json).toContain("\"metadataSource\":\"clob_v2\"");
    expect(order.ok).toBe(true);
    expect(order.status).toBe("matched");
    expect(order.venue_order_id).toBe("0xvenue-order");
    expect(order.accepted_size).toBe(10);
    expect(order.details_json).toContain("\"client_order_id\":\"client-1\"");
    expect(order.details_json).not.toContain("secret");
    expect(postedOrders).toHaveLength(1);
  });

  it("returns sanitized activity from CLOB trade recovery", async () => {
    const provider = createProvider();
    await provider.preflight(request);

    const activity = await provider.activity();

    expect(activity.user_stream_status).toBe("ok");
    expect(activity.user_stream_events).toEqual([]);
    expect(activity.clob_trades).toHaveLength(1);
    expect(activity.clob_trades[0]).toMatchObject({
      source: "clob_trades",
      event_type: "confirmed",
      market_id: "0xcondition",
      order_id: "0xtrade",
      trade_id: "0xtrade",
      asset_id: "up-token",
      side: "BUY",
      price: 0.5,
      size: 10,
      status: "confirmed",
    });
    expect(activity.details_json).toContain("\"clob_trade_count\":1");
    expect(activity.clob_trades[0].details_json).toContain("matched_amount");
    expect(activity.clob_trades[0].details_json).toContain("taker_order_id");
    expect(activity.details_json).not.toContain("secret");
  });

  it("rejects BUY market orders without dollar amount before reaching CLOB", async () => {
    const postedOrders: Array<Record<string, unknown>> = [];
    const provider = createProvider({ postedOrders });

    const order = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.51,
      size: 5,
      client_order_id: "client-missing-amount",
      details_json: null,
    });

    expect(order.ok).toBe(false);
    expect(order.status).toBe("validation_failed");
    expect(order.status_reason).toContain("amount_usd");
    expect(postedOrders).toHaveLength(0);
  });

  it("rejects resting and unticked orders before reaching CLOB", async () => {
    const provider = createProvider();
    const resting = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "GTC",
      limit_price: 0.51,
      size: 5,
      amount_usd: 5,
      client_order_id: "client-gtc",
      details_json: null,
    });
    const unticked = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 3,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.515,
      size: 5,
      amount_usd: 5,
      client_order_id: "client-unticked",
      details_json: null,
    });

    expect(resting.status).toBe("validation_failed");
    expect(resting.status_reason).toContain("FOK/FAK");
    expect(unticked.status).toBe("validation_failed");
    expect(unticked.status_reason).toContain("tick size");
  });

  it("rejects orders below venue min size before reaching CLOB", async () => {
    const postedOrders: Array<Record<string, unknown>> = [];
    const provider = createProvider({ postedOrders });

    const order = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.5,
      size: 4,
      amount_usd: 4,
      client_order_id: "client-below-min",
      details_json: null,
    });

    expect(order.ok).toBe(false);
    expect(order.status).toBe("validation_failed");
    expect(order.status_reason).toContain("below market min size");
    expect(postedOrders).toHaveLength(0);
  });

  it("returns CLOB rejections without treating them as transport success", async () => {
    const provider = createProvider({
      orderResponse: {
        success: false,
        status: "unmatched",
        errorMsg: "FOK_ORDER_NOT_FILLED_ERROR",
        orderID: "",
        takingAmount: "0",
        makingAmount: "0",
      },
    });

    const order = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.5,
      size: 10,
      amount_usd: 5,
      client_order_id: "client-clob-reject",
      details_json: null,
    });

    expect(order.ok).toBe(false);
    expect(order.status).toBe("unmatched");
    expect(order.status_reason).toBe("FOK_ORDER_NOT_FILLED_ERROR");
    expect(order.venue_order_id).toBeNull();
    expect(order.accepted_size).toBe(0);
  });

  it("fails SELL orders closed when token inventory is insufficient", async () => {
    const postedOrders: Array<Record<string, unknown>> = [];
    const provider = createProvider({
      postedOrders,
      positions: [{ asset: "up-token", size: "1", currentValue: 1 }],
    });

    const order = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "SELL",
      order_type: "FAK",
      limit_price: 0.5,
      size: 5,
      client_order_id: "client-sell-too-large",
      details_json: null,
    });

    expect(order.ok).toBe(false);
    expect(order.status).toBe("account_unavailable");
    expect(order.status_reason).toContain("Token inventory");
    expect(postedOrders).toHaveLength(0);
  });

  it("supports FAK partial fills and prevents duplicate client order submissions", async () => {
    const postedOrders: Array<Record<string, unknown>> = [];
    const provider = createProvider({
      postedOrders,
      orderResponse: {
        success: true,
        status: "partially_matched",
        takingAmount: "4000000",
        makingAmount: "2000000",
      },
    });
    const requestBody = {
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FAK",
      limit_price: 0.5,
      size: 10,
      amount_usd: 5,
      client_order_id: "client-partial",
      details_json: null,
    };

    const first = await provider.submitOrderIntent(requestBody);
    const second = await provider.submitOrderIntent(requestBody);

    expect(first.ok).toBe(true);
    expect(first.status).toBe("partially_matched");
    expect(first.accepted_size).toBe(4);
    expect(second.details_json).toContain("duplicate_client_order_id");
    expect(postedOrders).toHaveLength(1);
  });

  it("classifies matching-engine restart during order submission without faking success", async () => {
    const provider = createProvider({
      orderError: {
        value: Object.assign(new Error("engine restarting"), { status: 425 }),
      },
    });

    const order = await provider.submitOrderIntent({
      session_id: 1,
      intent_id: 2,
      market_id: "0xcondition",
      token_id: "up-token",
      side: "BUY",
      order_type: "FOK",
      limit_price: 0.5,
      size: 10,
      amount_usd: 5,
      client_order_id: "client-425",
      details_json: null,
    });

    expect(order.ok).toBe(false);
    expect(order.status).toBe("venue_restart");
    expect(order.status_reason).toContain("425");
  });

  it("normalizes CLOB cancel responses including not-canceled reasons", async () => {
    const provider = createProvider({
      cancelResponse: {
        canceled: ["0xorder-a"],
        not_canceled: { "0xorder-b": "already filled" },
      },
      cancelAllResponse: {
        canceled: ["0xorder-a", "0xorder-b"],
        not_canceled: {},
      },
    });

    const single = await provider.cancelOrder("0xorder-a");
    const all = await provider.cancelAll();

    expect(single.ok).toBe(false);
    expect(single.cancelled).toBe(1);
    expect(single.details_json).toContain("already filled");
    expect(all.ok).toBe(true);
    expect(all.cancelled).toBe(2);
  });

  it("classifies matching-engine restart during cancellation", async () => {
    const provider = createProvider({
      cancelError: {
        value: Object.assign(new Error("engine restarting"), { status: 425 }),
      },
      cancelAllError: {
        value: Object.assign(new Error("engine restarting"), { status: 425 }),
      },
    });

    const single = await provider.cancelOrder("0xorder-a");
    const all = await provider.cancelAll();

    expect(single.ok).toBe(false);
    expect(single.details_json).toContain("\"venue_restart\":true");
    expect(all.ok).toBe(false);
    expect(all.details_json).toContain("\"venue_restart\":true");
  });

  it("treats no redeemable positions as a safe no-op", async () => {
    const provider = createProvider({
      positions: [{ currentValue: 7, redeemable: false }],
    });

    const redemption = await provider.redeemAll();

    expect(redemption.ok).toBe(true);
    expect(redemption.submitted).toBe(0);
    expect(redemption.details_json).toContain("redeemable_condition_ids");
  });

  it("keeps redemption fail-closed when relayer auth cannot be used", async () => {
    const provider = createProvider({
      positions: [
        {
          currentValue: 7,
          redeemable: true,
          conditionId: "0xcondition",
          negativeRisk: false,
        },
      ],
    });

    const redemption = await provider.redeemAll();

    expect(redemption.ok).toBe(false);
    expect(redemption.submitted).toBe(0);
    expect(redemption.details_json).toContain("POLYMARKET_BUILDER_API_KEY");
  });

  it("returns failed redemption states without counting proceeds spendable", async () => {
    const provider = createProvider({
      builderCredentials: true,
      relayerTerminalState: "STATE_FAILED",
      positions: [
        {
          currentValue: 7,
          redeemable: true,
          conditionId: "0x00000000000000000000000000000000000000000000000000000000000000aa",
          negativeRisk: false,
        },
      ],
    });

    const redemption = await provider.redeemAll();

    expect(redemption.ok).toBe(false);
    expect(redemption.submitted).toBe(0);
    expect(redemption.details_json).toContain("STATE_FAILED");
  });

  it("returns unknown redemption states without counting proceeds spendable", async () => {
    const provider = createProvider({
      builderCredentials: true,
      relayerTerminalState: "STATE_NEW",
      positions: [
        {
          currentValue: 7,
          redeemable: true,
          conditionId: "0x00000000000000000000000000000000000000000000000000000000000000aa",
          negativeRisk: false,
        },
      ],
    });

    const redemption = await provider.redeemAll();

    expect(redemption.ok).toBe(false);
    expect(redemption.submitted).toBe(0);
    expect(redemption.details_json).toContain("STATE_NEW");
  });

  it("submits pUSD CTF redemptions when builder relayer credentials are configured", async () => {
    const relayerExecuteCalls: Array<Record<string, unknown>> = [];
    const provider = createProvider({
      builderCredentials: true,
      relayerExecuteCalls,
      positions: [
        {
          currentValue: 7,
          redeemable: true,
          conditionId: "0x00000000000000000000000000000000000000000000000000000000000000aa",
          negativeRisk: false,
        },
      ],
    });

    const redemption = await provider.redeemAll();

    expect(redemption.ok).toBe(true);
    expect(redemption.submitted).toBe(1);
    expect(redemption.details_json).toContain("STATE_CONFIRMED");
    expect(redemption.details_json).toContain("0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB");
    expect(relayerExecuteCalls).toHaveLength(1);
  });

  it("returns degraded discovery details when one market lookup fails", async () => {
    const provider = createProvider({ discoveryMode: "partial_failure" });
    const account = await provider.accountState();
    const health = await provider.health();
    const details = JSON.parse(account.details_json ?? "{}") as {
      discovery_degraded: boolean;
      discovery_error: string | null;
      active_markets: Array<{ conditionId: string }>;
    };

    expect(details.discovery_degraded).toBe(true);
    expect(details.discovery_error).toContain("market_discovery");
    expect(details.active_markets).toHaveLength(1);
    expect(health.readiness_status).toBe("degraded");
  });

  it("classifies CLOB V2 market metadata failures without faking readiness", async () => {
    const provider = createProvider({
      clobMarketInfoError: { value: new Error("clob metadata unavailable") },
    });
    const response = await provider.preflight(request);
    const health = await provider.health();
    const details = JSON.parse(response.details_json ?? "{}") as {
      discovery_degraded: boolean;
      discovery_error: string | null;
      active_markets: Array<{ metadataError: string | null; metadataSource: string }>;
    };

    expect(response.ok).toBe(false);
    expect(response.errors.join(" ")).toContain("market_metadata");
    expect(details.discovery_degraded).toBe(true);
    expect(details.discovery_error).toContain("market_metadata");
    expect(details.active_markets[0]?.metadataError).toContain(
      "clob metadata unavailable",
    );
    expect(details.active_markets[0]?.metadataSource).toBe("gamma");
    expect(health.readiness_status).toBe("degraded");
  });

  it("surfaces matching-engine restart responses as retryable venue degradation", async () => {
    const provider = createProvider({
      clobMarketInfoError: {
        value: Object.assign(new Error("venue unavailable"), { status: 425 }),
      },
    });
    const response = await provider.preflight(request);

    expect(response.ok).toBe(false);
    expect(response.errors.join(" ")).toContain("425");
    expect(response.errors.join(" ")).toContain("matching engine restart");
  });

  it("fails closed when core account decomposition cannot be read", async () => {
    vi.useFakeTimers();
    const provider = createProvider({ httpTimeoutOnPositions: true });
    const accountPromise = provider.accountState().catch((error) => error as Error);

    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(20);
    const error = (await accountPromise) as Error;
    expect(error.message).toContain("positions: request timed out after 10ms");

    const health = await provider.health();
    expect(health.last_account_refresh_error).toContain("positions:");
    expect(health.ready).toBe(false);
  });

  it("clears degraded account health after a successful recovery", async () => {
    vi.useFakeTimers();
    const ensureConnectedError: { value: string | null } = {
      value: "stream down",
    };
    const provider = createProvider({ ensureConnectedError });

    await provider.preflight(request);
    let health = await provider.health();
    expect(health.readiness_status).toBe("degraded");

    ensureConnectedError.value = null;
    await provider.accountState();
    health = await provider.health();

    expect(health.readiness_status).toBe("ready");
    expect(health.last_account_refresh_error).toBeNull();
  });
});
