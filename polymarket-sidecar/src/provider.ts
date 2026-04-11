import { Wallet } from "@ethersproject/wallet";
import {
  Chain,
  ClobClient,
  type ApiKeyCreds,
  type BalanceAllowanceResponse,
  type OpenOrder,
} from "@polymarket/clob-client";
import WebSocket, { type RawData } from "ws";
import type {
  LiveAccountState,
  LiveCancellationResponse,
  LiveOrderIntentRequest,
  LiveOrderIntentResponse,
  LivePreflightRequest,
  LivePreflightResponse,
  LiveRedemptionResponse,
} from "./types.js";
import type { SidecarConfig } from "./config.js";

const DEFAULT_GAMMA_API_URL = "https://gamma-api.polymarket.com";
const DEFAULT_DATA_API_URL = "https://data-api.polymarket.com";
const USER_STREAM_URL = "wss://ws-subscriptions-clob.polymarket.com/ws/user";
const USER_STREAM_CONNECT_TIMEOUT_MS = 5_000;
const USER_STREAM_STABLE_GRACE_MS = 1_000;
const USER_STREAM_RECONNECT_DELAY_MS = 1_000;
const CHAIN_ID = Chain.POLYGON;

type LiveCheckStatus = "ok" | "failed";

interface SidecarHealthResponse {
  ok: boolean;
  mode: string;
  provider: string;
  signature_type: number;
  wallet_address: string | null;
  proxy_wallet: string | null;
  auth_configured: boolean;
  relayer_api_key_present: boolean;
  user_stream_status: LiveCheckStatus;
  last_user_stream_connected_at_ms: number | null;
  last_user_stream_event_at_ms: number | null;
  last_user_stream_error: string | null;
}

interface GeoblockResponse {
  blocked?: boolean;
  country?: string;
  ip?: string;
  region?: string;
}

interface GammaMarketEntry {
  id?: string;
  slug?: string;
  conditionId?: string;
  condition_id?: string;
  orderMinSize?: number | string;
  orderPriceMinTickSize?: number | string;
  acceptingOrders?: boolean;
}

interface GammaEventResponse {
  slug?: string;
  markets?: GammaMarketEntry[];
}

interface PositionResponse {
  proxyWallet?: string;
  currentValue?: number | string;
  redeemable?: boolean;
}

interface ActiveMarket {
  slug: string;
  conditionId: string;
  minOrderSize: number | null;
  tickSize: number | null;
  acceptingOrders: boolean;
}

interface ActiveMarketDiscovery {
  markets: ActiveMarket[];
  legalOrderMinUsd: number | null;
}

interface UserStreamSnapshot {
  status: LiveCheckStatus;
  lastConnectedAtMs: number | null;
  lastEventAtMs: number | null;
  lastError: string | null;
  subscribedMarkets: string[];
}

interface UserStreamMonitor {
  ensureConnected(auth: ApiKeyCreds, markets: string[]): Promise<void>;
  snapshot(): UserStreamSnapshot;
  close(): void;
}

interface ClobReadonlyClient {
  createOrDeriveApiKey(): Promise<ApiKeyCreds>;
  getServerTime(): Promise<number>;
  getBalanceAllowance(): Promise<BalanceAllowanceResponse>;
  getOpenOrders(): Promise<OpenOrder[]>;
}

interface AuthState {
  creds: ApiKeyCreds;
  client: ClobReadonlyClient;
  walletAddress: string | null;
  proxyWallet: string | null;
}

interface AccountObservation {
  state: LiveAccountState;
  allowanceStatus: LiveCheckStatus;
  legalOrderMinUsd: number | null;
  openOrderCount: number;
  positionCount: number;
}

interface ProviderDeps {
  fetchImpl: typeof fetch;
  nowMs: () => number;
  createClobClient: (config: SidecarConfig, creds?: ApiKeyCreds) => ClobReadonlyClient;
  createUserStreamMonitor: (nowMs: () => number) => UserStreamMonitor;
}

export interface SidecarProvider {
  health(): Promise<SidecarHealthResponse>;
  preflight(request: LivePreflightRequest): Promise<LivePreflightResponse>;
  accountState(): Promise<LiveAccountState>;
  submitOrderIntent(
    request: LiveOrderIntentRequest,
  ): Promise<LiveOrderIntentResponse>;
  cancelOrder(orderId: string): Promise<LiveCancellationResponse>;
  cancelAll(): Promise<LiveCancellationResponse>;
  redeemAll(): Promise<LiveRedemptionResponse>;
}

function nowMs(): number {
  return Date.now();
}

function checkStatus(ok: boolean): LiveCheckStatus {
  return ok ? "ok" : "failed";
}

function walletAddress(config: SidecarConfig): string | null {
  return config.funder ?? config.proxyWallet;
}

function hasAuthCredentials(config: SidecarConfig): boolean {
  return Boolean(config.privateKey && config.proxyWallet && config.funder);
}

function parseNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function normalizeServerTimeMs(value: number): number {
  return value >= 1_000_000_000_000 ? value : value * 1_000;
}

function currentSlotSlugs(nowMsValue: number): string[] {
  const epochSecs = Math.floor(nowMsValue / 1_000);
  const currentSlot = Math.floor(epochSecs / 300) * 300;
  return [
    `btc-updown-5m-${currentSlot}`,
    `btc-updown-5m-${currentSlot + 300}`,
  ];
}

function parseActiveMarkets(body: unknown): ActiveMarket[] {
  const event = (body ?? {}) as GammaEventResponse;
  const slug = typeof event.slug === "string" ? event.slug : "";
  const markets = Array.isArray(event.markets) ? event.markets : [];
  return markets
    .map((market) => {
      const conditionId =
        typeof market.conditionId === "string"
          ? market.conditionId
          : typeof market.condition_id === "string"
            ? market.condition_id
            : typeof market.id === "string"
              ? market.id
              : null;
      if (!conditionId) return null;
      return {
        slug,
        conditionId,
        minOrderSize: parseNumber(market.orderMinSize),
        tickSize: parseNumber(market.orderPriceMinTickSize),
        acceptingOrders: market.acceptingOrders !== false,
      } satisfies ActiveMarket;
    })
    .filter((market): market is ActiveMarket => market !== null);
}

function dedupeStrings(values: string[]): string[] {
  return [...new Set(values)];
}

function inferDataApiUrl(gammaApiUrl: string): string {
  try {
    const parsed = new URL(gammaApiUrl);
    parsed.hostname = parsed.hostname.replace("gamma-api.", "data-api.");
    if (parsed.hostname === new URL(gammaApiUrl).hostname) {
      return DEFAULT_DATA_API_URL;
    }
    parsed.pathname = "";
    parsed.search = "";
    parsed.hash = "";
    return parsed.toString().replace(/\/$/, "");
  } catch {
    return DEFAULT_DATA_API_URL;
  }
}

function sumReservedBuyCash(openOrders: OpenOrder[]): number {
  return openOrders.reduce((total, order) => {
    if (order.side !== "BUY") return total;
    const originalSize = parseNumber(order.original_size) ?? 0;
    const matchedSize = parseNumber(order.size_matched) ?? 0;
    const remainingSize = Math.max(originalSize - matchedSize, 0);
    const price = parseNumber(order.price) ?? 0;
    return total + remainingSize * price;
  }, 0);
}

function legalOrderMinUsd(markets: ActiveMarket[]): number | null {
  const values = markets
    .filter((market) => market.acceptingOrders)
    .map((market) => market.minOrderSize)
    .filter((value): value is number => value != null && value > 0);
  if (values.length === 0) return null;
  return Math.min(...values);
}

class WsUserStreamMonitor implements UserStreamMonitor {
  private socket: WebSocket | null = null;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private desired: { auth: ApiKeyCreds; markets: string[] } | null = null;
  private readonly state: UserStreamSnapshot = {
    status: "failed",
    lastConnectedAtMs: null,
    lastEventAtMs: null,
    lastError: null,
    subscribedMarkets: [],
  };

  constructor(private readonly now: () => number) {}

  async ensureConnected(auth: ApiKeyCreds, markets: string[]): Promise<void> {
    const desiredMarkets = dedupeStrings(markets).sort();
    if (desiredMarkets.length === 0) {
      throw new Error("no market ids available for the authenticated user stream");
    }
    this.desired = { auth, markets: desiredMarkets };
    if (
      this.socket &&
      this.state.status === "ok" &&
      JSON.stringify(this.state.subscribedMarkets) === JSON.stringify(desiredMarkets)
    ) {
      return;
    }
    await this.connect(desiredMarkets, auth);
  }

  snapshot(): UserStreamSnapshot {
    return { ...this.state, subscribedMarkets: [...this.state.subscribedMarkets] };
  }

  close(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.socket) {
      this.socket.removeAllListeners();
      this.socket.close();
      this.socket = null;
    }
    this.state.status = "failed";
  }

  private async connect(markets: string[], auth: ApiKeyCreds): Promise<void> {
    this.close();
    await new Promise<void>((resolve, reject) => {
      const socket = new WebSocket(USER_STREAM_URL);
      let settled = false;
      let readyTimer: NodeJS.Timeout | null = null;
      let connectTimer: NodeJS.Timeout | null = setTimeout(() => {
        fail(new Error("timed out connecting to the authenticated user stream"));
      }, USER_STREAM_CONNECT_TIMEOUT_MS);

      const succeed = (): void => {
        if (settled) return;
        settled = true;
        if (connectTimer) clearTimeout(connectTimer);
        if (readyTimer) clearTimeout(readyTimer);
        this.socket = socket;
        this.state.status = "ok";
        this.state.lastConnectedAtMs = this.now();
        this.state.lastError = null;
        this.state.subscribedMarkets = [...markets];
        resolve();
      };

      const fail = (error: Error): void => {
        if (settled) return;
        settled = true;
        if (connectTimer) clearTimeout(connectTimer);
        if (readyTimer) clearTimeout(readyTimer);
        this.state.status = "failed";
        this.state.lastError = error.message;
        socket.removeAllListeners();
        socket.close();
        reject(error);
      };

      socket.on("open", () => {
        socket.send(
          JSON.stringify({
            auth: {
              apiKey: auth.key,
              secret: auth.secret,
              passphrase: auth.passphrase,
            },
            markets,
            type: "user",
          }),
        );
        readyTimer = setTimeout(() => succeed(), USER_STREAM_STABLE_GRACE_MS);
      });

      socket.on("message", (data: RawData) => {
        this.state.lastEventAtMs = this.now();
        const message = data.toString();
        try {
          const parsed = JSON.parse(message) as Record<string, unknown>;
          if (
            !settled &&
            ((parsed.type === "error" && typeof parsed.error === "string") ||
              (parsed.event_type === "error" && typeof parsed.error === "string"))
          ) {
            fail(new Error(parsed.error as string));
            return;
          }
        } catch {
          void 0;
        }
        if (!settled) {
          succeed();
        }
      });

      socket.on("error", (error: Error) => {
        const err = error instanceof Error ? error : new Error(String(error));
        if (!settled) {
          fail(err);
          return;
        }
        this.markDisconnected(err.message);
      });

      socket.on("close", (code: number, reason: Buffer) => {
        const message = `user stream closed (${code}): ${reason.toString()}`;
        if (!settled) {
          fail(new Error(message));
          return;
        }
        this.markDisconnected(message);
      });
    });
  }

  private markDisconnected(message: string): void {
    this.state.status = "failed";
    this.state.lastError = message;
    this.socket = null;
    if (!this.desired || this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      const desired = this.desired;
      if (!desired) return;
      void this.connect(desired.markets, desired.auth).catch((error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        this.markDisconnected(message);
      });
    }, USER_STREAM_RECONNECT_DELAY_MS);
  }
}

function defaultProviderDeps(): ProviderDeps {
  return {
    fetchImpl: fetch,
    nowMs,
    createClobClient: (config, creds) => {
      if (!config.privateKey) {
        throw new Error(
          "POLYMARKET_PRIVATE_KEY is required for authenticated Polymarket readonly access",
        );
      }
      const signer = new Wallet(config.privateKey);
      return new ClobClient(
        config.clobHost,
        CHAIN_ID,
        signer,
        creds,
        config.signatureType,
        walletAddress(config) ?? undefined,
        undefined,
        true,
        undefined,
        undefined,
        undefined,
        undefined,
        true,
      );
    },
    createUserStreamMonitor: (clock) => new WsUserStreamMonitor(clock),
  };
}

export class StubSidecarProvider implements SidecarProvider {
  constructor(private readonly config: SidecarConfig) {}

  async health(): Promise<SidecarHealthResponse> {
    return {
      ok: true,
      mode: "local-sidecar",
      provider: "stub",
      signature_type: this.config.signatureType,
      wallet_address: walletAddress(this.config),
      proxy_wallet: this.config.proxyWallet,
      auth_configured: hasAuthCredentials(this.config),
      relayer_api_key_present: Boolean(this.config.relayerApiKey),
      user_stream_status: "failed",
      last_user_stream_connected_at_ms: null,
      last_user_stream_event_at_ms: null,
      last_user_stream_error: "stub provider",
    };
  }

  async preflight(request: LivePreflightRequest): Promise<LivePreflightResponse> {
    const authOk = hasAuthCredentials(this.config);
    const availableCashUsd = null;
    const legalOrderMinUsd = 5;
    const errors: string[] = [];
    if (!authOk) {
      errors.push(
        "Missing POLY_PROXY credentials. Set POLYMARKET_PRIVATE_KEY, POLYMARKET_PROXY_WALLET, and POLYMARKET_FUNDER.",
      );
    }
    errors.push(
      "Stub sidecar cannot verify live venue state yet. Treat this preflight as contract validation only until the real provider replaces the stub.",
    );
    if (request.budget_limits.min_required_cash_usd > request.budget_limits.cash_cap_usd) {
      errors.push("Configured live cash cap is below the minimum required cash.");
    }

    return {
      ok: errors.length === 0,
      mode: request.execution_mode,
      wallet_address: walletAddress(this.config),
      proxy_wallet: this.config.proxyWallet,
      geoblock_status: "failed",
      geoblock_country_code: null,
      auth_status: checkStatus(authOk),
      clock_status: "failed",
      allowance_status: "failed",
      user_stream_status: "failed",
      available_cash_usd: availableCashUsd,
      legal_order_min_usd: legalOrderMinUsd,
      details_json: JSON.stringify({
        provider: "stub",
        strategy_readiness: request.strategy_readiness,
        relayer_host: this.config.relayerHost,
        wallet_address_source: this.config.funder ? "funder" : "proxy_wallet",
      }),
      errors,
    };
  }

  async accountState(): Promise<LiveAccountState> {
    return {
      timestamp_ms: nowMs(),
      wallet_address: walletAddress(this.config),
      proxy_wallet: this.config.proxyWallet,
      cash_available: 0,
      cash_reserved_for_orders: 0,
      inventory_mark_value: 0,
      redeemable_value: 0,
      pending_redeem_value: 0,
      total_equity: 0,
      allowance_available: 0,
      details_json: JSON.stringify({
        provider: "stub",
        account_state_not_verified: true,
        relayer_api_key_present: Boolean(this.config.relayerApiKey),
      }),
    };
  }

  async submitOrderIntent(
    request: LiveOrderIntentRequest,
  ): Promise<LiveOrderIntentResponse> {
    return {
      ok: false,
      venue_order_id: null,
      client_order_id: request.client_order_id,
      status: "not_implemented",
      status_reason: "Live order routing is intentionally disabled in this pass.",
      accepted_size: null,
      details_json: JSON.stringify({ provider: "stub" }),
    };
  }

  async cancelOrder(_orderId: string): Promise<LiveCancellationResponse> {
    return {
      ok: false,
      cancelled: 0,
      details_json: JSON.stringify({ provider: "stub", reason: "cancel not implemented" }),
    };
  }

  async cancelAll(): Promise<LiveCancellationResponse> {
    return {
      ok: false,
      cancelled: 0,
      details_json: JSON.stringify({ provider: "stub", reason: "cancel not implemented" }),
    };
  }

  async redeemAll(): Promise<LiveRedemptionResponse> {
    return {
      ok: false,
      submitted: 0,
      details_json: JSON.stringify({ provider: "stub", reason: "redemption not implemented" }),
    };
  }
}

export class PolymarketReadonlyProvider implements SidecarProvider {
  private readonly deps: ProviderDeps;
  private readonly userStream: UserStreamMonitor;
  private authStatePromise: Promise<AuthState> | null = null;
  private lastGammaApiUrl: string = DEFAULT_GAMMA_API_URL;
  private lastDiscovery: ActiveMarket[] = [];

  constructor(
    private readonly config: SidecarConfig,
    deps: Partial<ProviderDeps> = {},
  ) {
    const defaults = defaultProviderDeps();
    this.deps = {
      fetchImpl: deps.fetchImpl ?? defaults.fetchImpl,
      nowMs: deps.nowMs ?? defaults.nowMs,
      createClobClient: deps.createClobClient ?? defaults.createClobClient,
      createUserStreamMonitor:
        deps.createUserStreamMonitor ?? defaults.createUserStreamMonitor,
    };
    this.userStream = this.deps.createUserStreamMonitor(this.deps.nowMs);
  }

  async health(): Promise<SidecarHealthResponse> {
    const stream = this.userStream.snapshot();
    return {
      ok: true,
      mode: "local-sidecar",
      provider: "polymarket",
      signature_type: this.config.signatureType,
      wallet_address: walletAddress(this.config),
      proxy_wallet: this.config.proxyWallet,
      auth_configured: hasAuthCredentials(this.config),
      relayer_api_key_present: Boolean(this.config.relayerApiKey),
      user_stream_status: stream.status,
      last_user_stream_connected_at_ms: stream.lastConnectedAtMs,
      last_user_stream_event_at_ms: stream.lastEventAtMs,
      last_user_stream_error: stream.lastError,
    };
  }

  async preflight(request: LivePreflightRequest): Promise<LivePreflightResponse> {
    this.lastGammaApiUrl = request.gamma_api_url;

    const errors: string[] = [];
    const geoblock = await this.fetchGeoblock().catch((error: unknown) => {
      errors.push(`Geoblock check failed: ${stringError(error)}`);
      return null;
    });
    const geoblockOk = geoblock?.blocked === false;

    let authStatus: LiveCheckStatus = "failed";
    let clockStatus: LiveCheckStatus = "failed";
    let allowanceStatus: LiveCheckStatus = "failed";
    let userStreamStatus: LiveCheckStatus = "failed";
    let availableCashUsd: number | null = null;
    let legalOrderMinUsd: number | null = null;
    let authState: AuthState | null = null;
    let discovery: ActiveMarketDiscovery | null = null;

    try {
      authState = await this.getAuthState();
      authStatus = "ok";
    } catch (error) {
      errors.push(`Authentication failed: ${stringError(error)}`);
    }

    if (authState) {
      try {
        const serverTimeMs = normalizeServerTimeMs(
          await authState.client.getServerTime(),
        );
        const driftMs = Math.abs(serverTimeMs - this.deps.nowMs());
        clockStatus = checkStatus(driftMs <= this.config.clockDriftMaxMs);
        if (clockStatus === "failed") {
          errors.push(
            `Clock drift ${driftMs}ms exceeds POLYMARKET_CLOCK_DRIFT_MAX_MS ${this.config.clockDriftMaxMs}ms.`,
          );
        }
      } catch (error) {
        errors.push(`Clock check failed: ${stringError(error)}`);
      }

      try {
        discovery = await this.discoverActiveMarkets(request.gamma_api_url);
        this.lastDiscovery = discovery.markets;
        legalOrderMinUsd = discovery.legalOrderMinUsd;
        if (discovery.markets.length === 0) {
          errors.push("No active BTC 5-minute markets were discovered from Gamma.");
        }
      } catch (error) {
        errors.push(`Active-market discovery failed: ${stringError(error)}`);
      }

      if (discovery && discovery.markets.length > 0) {
        try {
          await this.userStream.ensureConnected(
            authState.creds,
            discovery.markets.map((market) => market.conditionId),
          );
          userStreamStatus = "ok";
        } catch (error) {
          errors.push(`Authenticated user stream failed: ${stringError(error)}`);
        }
      }

      try {
        const account = await this.observeAccountState(
          authState,
          discovery?.markets ?? [],
          request.gamma_api_url,
        );
        allowanceStatus = account.allowanceStatus;
        availableCashUsd = account.state.cash_available;
        legalOrderMinUsd = legalOrderMinUsd ?? account.legalOrderMinUsd;
      } catch (error) {
        errors.push(`Account state could not be read: ${stringError(error)}`);
      }
    }

    if (
      availableCashUsd != null &&
      availableCashUsd < request.budget_limits.min_required_cash_usd
    ) {
      errors.push(
        `Available cash ${availableCashUsd.toFixed(2)} is below LIVE_MIN_REQUIRED_CASH_USD ${request.budget_limits.min_required_cash_usd.toFixed(2)}.`,
      );
    }

    if (
      legalOrderMinUsd != null &&
      legalOrderMinUsd > request.budget_limits.max_single_order_usd
    ) {
      errors.push(
        `Legal order minimum ${legalOrderMinUsd.toFixed(2)} exceeds LIVE_MAX_SINGLE_ORDER_USD ${request.budget_limits.max_single_order_usd.toFixed(2)}.`,
      );
    }

    if (
      availableCashUsd != null &&
      legalOrderMinUsd != null &&
      availableCashUsd < legalOrderMinUsd
    ) {
      errors.push(
        `Available cash ${availableCashUsd.toFixed(2)} is below the current legal order minimum ${legalOrderMinUsd.toFixed(2)}.`,
      );
    }

    const stream = this.userStream.snapshot();
    return {
      ok: errors.length === 0,
      mode: request.execution_mode,
      wallet_address: walletAddress(this.config),
      proxy_wallet: this.config.proxyWallet,
      geoblock_status: checkStatus(geoblockOk),
      geoblock_country_code: geoblock?.country ?? null,
      auth_status: authStatus,
      clock_status: clockStatus,
      allowance_status: allowanceStatus,
      user_stream_status: userStreamStatus,
      available_cash_usd: availableCashUsd,
      legal_order_min_usd: legalOrderMinUsd,
      details_json: JSON.stringify({
        provider: "polymarket",
        strategy_readiness: request.strategy_readiness,
        relayer_api_key_present: Boolean(this.config.relayerApiKey),
        geoblock,
        active_markets: this.lastDiscovery,
        wallet_address_source: this.config.funder ? "funder" : "proxy_wallet",
        last_user_stream_connected_at_ms: stream.lastConnectedAtMs,
        last_user_stream_event_at_ms: stream.lastEventAtMs,
        last_user_stream_error: stream.lastError,
      }),
      errors,
    };
  }

  async accountState(): Promise<LiveAccountState> {
    const authState = await this.getAuthState();
    const gammaApiUrl = this.lastGammaApiUrl || DEFAULT_GAMMA_API_URL;
    const discovery = await this.discoverActiveMarkets(gammaApiUrl).catch(() => ({
      markets: this.lastDiscovery,
      legalOrderMinUsd: legalOrderMinUsd(this.lastDiscovery),
    }));
    this.lastDiscovery = discovery.markets;

    if (discovery.markets.length > 0) {
      await this.userStream
        .ensureConnected(
          authState.creds,
          discovery.markets.map((market) => market.conditionId),
        )
        .catch(() => undefined);
    }

    const observation = await this.observeAccountState(
      authState,
      discovery.markets,
      gammaApiUrl,
    );
    return observation.state;
  }

  async submitOrderIntent(
    request: LiveOrderIntentRequest,
  ): Promise<LiveOrderIntentResponse> {
    return {
      ok: false,
      venue_order_id: null,
      client_order_id: request.client_order_id,
      status: "not_implemented",
      status_reason: "Live order routing remains intentionally disabled in this pass.",
      accepted_size: null,
      details_json: JSON.stringify({ provider: "polymarket", mode: "readonly_only" }),
    };
  }

  async cancelOrder(_orderId: string): Promise<LiveCancellationResponse> {
    return {
      ok: false,
      cancelled: 0,
      details_json: JSON.stringify({
        provider: "polymarket",
        reason: "cancel not implemented",
      }),
    };
  }

  async cancelAll(): Promise<LiveCancellationResponse> {
    return {
      ok: false,
      cancelled: 0,
      details_json: JSON.stringify({
        provider: "polymarket",
        reason: "cancel not implemented",
      }),
    };
  }

  async redeemAll(): Promise<LiveRedemptionResponse> {
    return {
      ok: false,
      submitted: 0,
      details_json: JSON.stringify({
        provider: "polymarket",
        reason: "redemption not implemented",
      }),
    };
  }

  private async getAuthState(): Promise<AuthState> {
    if (!this.authStatePromise) {
      this.authStatePromise = (async () => {
        if (!hasAuthCredentials(this.config)) {
          throw new Error(
            "Missing POLY_PROXY credentials. Set POLYMARKET_PRIVATE_KEY, POLYMARKET_PROXY_WALLET, and POLYMARKET_FUNDER.",
          );
        }
        const bootstrapClient = this.deps.createClobClient(this.config);
        const creds = await bootstrapClient.createOrDeriveApiKey();
        return {
          creds,
          client: this.deps.createClobClient(this.config, creds),
          walletAddress: walletAddress(this.config),
          proxyWallet: this.config.proxyWallet,
        } satisfies AuthState;
      })();
    }
    return this.authStatePromise;
  }

  private async fetchGeoblock(): Promise<GeoblockResponse> {
    const response = await this.deps.fetchImpl(this.config.geoblockUrl);
    if (!response.ok) {
      throw new Error(`geoblock endpoint returned ${response.status}`);
    }
    return (await response.json()) as GeoblockResponse;
  }

  private async discoverActiveMarkets(
    gammaApiUrl: string,
  ): Promise<ActiveMarketDiscovery> {
    const markets: ActiveMarket[] = [];
    for (const slug of currentSlotSlugs(this.deps.nowMs())) {
      const response = await this.deps.fetchImpl(`${gammaApiUrl}/events/slug/${slug}`);
      if (response.status === 404) continue;
      if (!response.ok) {
        throw new Error(`gamma discovery returned ${response.status} for ${slug}`);
      }
      const parsed = parseActiveMarkets(await response.json());
      markets.push(...parsed);
    }
    const deduped = dedupeStrings(markets.map((market) => market.conditionId)).map(
      (conditionId) =>
        markets.find((market) => market.conditionId === conditionId) as ActiveMarket,
    );
    return {
      markets: deduped,
      legalOrderMinUsd: legalOrderMinUsd(deduped),
    };
  }

  private async fetchPositions(
    wallet: string,
    gammaApiUrl: string,
  ): Promise<PositionResponse[]> {
    const dataApiUrl = inferDataApiUrl(gammaApiUrl);
    const url = new URL(`${dataApiUrl}/positions`);
    url.searchParams.set("user", wallet);
    url.searchParams.set("sizeThreshold", "0");
    url.searchParams.set("limit", "500");
    const response = await this.deps.fetchImpl(url);
    if (!response.ok) {
      throw new Error(`positions endpoint returned ${response.status}`);
    }
    const parsed = await response.json();
    return Array.isArray(parsed) ? (parsed as PositionResponse[]) : [];
  }

  private async observeAccountState(
    authState: AuthState,
    markets: ActiveMarket[],
    gammaApiUrl: string,
  ): Promise<AccountObservation> {
    if (!authState.walletAddress) {
      throw new Error("wallet address is missing");
    }

    const [balanceAllowance, openOrders, positions] = await Promise.all([
      authState.client.getBalanceAllowance(),
      authState.client.getOpenOrders(),
      this.fetchPositions(authState.walletAddress, gammaApiUrl),
    ]);

    const rawBalance = parseNumber(balanceAllowance.balance) ?? 0;
    const rawAllowance = parseNumber(balanceAllowance.allowance);
    const cashReservedForOrders = sumReservedBuyCash(openOrders);
    const inventoryMarkValue = positions
      .filter((position) => position.redeemable !== true)
      .reduce((total, position) => total + (parseNumber(position.currentValue) ?? 0), 0);
    const redeemableValue = positions
      .filter((position) => position.redeemable === true)
      .reduce((total, position) => total + (parseNumber(position.currentValue) ?? 0), 0);
    const cashAvailable = Math.max(rawBalance - cashReservedForOrders, 0);
    const allowanceAvailable =
      rawAllowance == null
        ? null
        : Math.max(Math.min(rawAllowance, rawBalance) - cashReservedForOrders, 0);
    const stream = this.userStream.snapshot();
    const legalMinUsd = legalOrderMinUsd(markets);
    const accountState: LiveAccountState = {
      timestamp_ms: this.deps.nowMs(),
      wallet_address: authState.walletAddress,
      proxy_wallet: authState.proxyWallet,
      cash_available: cashAvailable,
      cash_reserved_for_orders: cashReservedForOrders,
      inventory_mark_value: inventoryMarkValue,
      redeemable_value: redeemableValue,
      pending_redeem_value: 0,
      total_equity: rawBalance + inventoryMarkValue + redeemableValue,
      allowance_available: allowanceAvailable,
      details_json: JSON.stringify({
        provider: "polymarket",
        relayer_api_key_present: Boolean(this.config.relayerApiKey),
        user_stream_status: stream.status,
        last_user_stream_connected_at_ms: stream.lastConnectedAtMs,
        last_user_stream_event_at_ms: stream.lastEventAtMs,
        last_successful_account_refresh_at_ms: this.deps.nowMs(),
        account_refresh_error: null,
        legal_order_min_usd: legalMinUsd,
        open_order_count: openOrders.length,
        open_buy_order_count: openOrders.filter((order) => order.side === "BUY").length,
        open_sell_order_count: openOrders.filter((order) => order.side === "SELL").length,
        position_count: positions.length,
        redeemable_position_count: positions.filter((position) => position.redeemable === true)
          .length,
        observed_balance_usd: rawBalance,
        observed_allowance_usd: rawAllowance,
        active_markets: markets,
      }),
    };
    return {
      state: accountState,
      allowanceStatus: checkStatus((allowanceAvailable ?? 0) > 0),
      legalOrderMinUsd: legalMinUsd,
      openOrderCount: openOrders.length,
      positionCount: positions.length,
    };
  }
}

function stringError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createDefaultProvider(
  config: SidecarConfig,
): SidecarProvider {
  return new PolymarketReadonlyProvider(config);
}
