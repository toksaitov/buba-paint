import { Wallet } from "@ethersproject/wallet";
import https from "node:https";
import {
  AssetType,
  Chain,
  ClobClient,
  OrderType,
  SignatureTypeV2,
  Side,
  createL1Headers,
  createL2Headers,
  type ApiKeyCreds,
  type MarketDetails,
  type OpenOrder,
  type OrderResponse,
  type TickSize,
  type Trade,
  type UserMarketOrderV2,
} from "@polymarket/clob-client-v2";
import builderRelayerClient from "@polymarket/builder-relayer-client";
import { BuilderConfig } from "@polymarket/builder-signing-sdk";
import {
  encodeFunctionData,
  zeroHash,
  type Hex,
} from "viem";
import WebSocket, { type RawData } from "ws";
import type {
  RelayerTransaction,
  RelayerTransactionResponse,
  RelayerTxType as RelayerTxTypeValue,
  Transaction,
} from "@polymarket/builder-relayer-client";
import type {
  LiveAccountState,
  LiveActivityEvent,
  LiveActivityResponse,
  LiveCancellationResponse,
  LiveOrderIntentRequest,
  LiveOrderIntentResponse,
  LivePreflightRequest,
  LivePreflightResponse,
  LiveRedemptionResponse,
} from "./types.js";
import type { SidecarConfig } from "./config.js";
import { logEvent, type LogLevel } from "./logging.js";

const DEFAULT_GAMMA_API_URL = "https://gamma-api.polymarket.com";
const DEFAULT_DATA_API_URL = "https://data-api.polymarket.com";
const USER_STREAM_URL = "wss://ws-subscriptions-clob.polymarket.com/ws/user";
const CHAIN_ID = Chain.POLYGON;
const COLLATERAL_DECIMALS = 1_000_000;
const POLYMARKET_HTTP_USER_AGENT = "buba-polymarket-sidecar/0.1.0";
const PUSD_COLLATERAL_ADDRESS = "0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB";
const CTF_ADDRESS = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045";
const ZERO_BYTES32 = zeroHash;
const CLOB_INITIAL_CURSOR = "MA==";
const CLOB_END_CURSOR = "LTE=";
const USER_STREAM_PING_INTERVAL_MS = 10_000;

const {
  RelayClient,
  RelayerTxType,
  RelayerTransactionState,
} = builderRelayerClient as typeof import("@polymarket/builder-relayer-client");

type LiveCheckStatus = "ok" | "failed";
type ReadinessStatus = "ready" | "degraded" | "failed";
type SidecarErrorStage =
  | "geoblock_check"
  | "auth_bootstrap"
  | "clock_check"
  | "market_discovery"
  | "market_metadata"
  | "user_stream_connect"
  | "balance_allowance"
  | "open_orders"
  | "positions"
  | "activity"
  | "order_submission"
  | "cancel_order"
  | "cancel_all"
  | "redemption";
type UserStreamLifecycle =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed";

interface SidecarHealthResponse {
  ok: boolean;
  ready: boolean;
  readiness_status: ReadinessStatus;
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
  last_successful_account_refresh_at_ms: number | null;
  last_account_refresh_error: string | null;
  last_user_stream_disconnect_at_ms: number | null;
  last_user_stream_disconnect_reason: string | null;
  consecutive_user_stream_failures: number;
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
  asset?: string;
  conditionId?: string;
  condition_id?: string;
  size?: number | string;
  currentValue?: number | string;
  redeemable?: boolean;
  negativeRisk?: boolean;
  outcomeIndex?: number;
}

interface BalanceAllowanceView {
  balance: string;
  allowance?: string;
  allowances?: Record<string, string>;
}

interface ActiveMarketToken {
  tokenId: string;
  outcome: string;
}

interface ActiveMarketFeeDetails {
  rate: number | null;
  exponent: number | null;
  takerOnly: boolean | null;
  makerBaseFee: number | null;
  takerBaseFee: number | null;
}

interface ActiveMarket {
  slug: string;
  conditionId: string;
  minOrderSize: number | null;
  tickSize: number | null;
  acceptingOrders: boolean;
  tokens: ActiveMarketToken[];
  feeDetails: ActiveMarketFeeDetails | null;
  negRisk: boolean | null;
  metadataSource: "gamma" | "clob_v2";
  metadataError: string | null;
}

interface ActiveMarketDiscovery {
  markets: ActiveMarket[];
  legalOrderMinUsd: number | null;
  degraded: boolean;
  error: string | null;
  usedFallback: boolean;
}

interface UserStreamSnapshot {
  status: LiveCheckStatus;
  lifecycle: UserStreamLifecycle;
  lastConnectedAtMs: number | null;
  lastEventAtMs: number | null;
  lastError: string | null;
  lastDisconnectedAtMs: number | null;
  lastDisconnectReason: string | null;
  consecutiveFailures: number;
  subscribedMarkets: string[];
  recentEvents: LiveActivityEvent[];
}

interface UserStreamMonitor {
  ensureConnected(auth: ApiKeyCreds, markets: string[]): Promise<void>;
  snapshot(): UserStreamSnapshot;
  close(): void;
}

interface ClobVenueClient {
  createOrDeriveApiKey(): Promise<ApiKeyCreds>;
  getServerTime(): Promise<number>;
  getBalanceAllowance(): Promise<BalanceAllowanceView>;
  getOpenOrders(): Promise<OpenOrder[]>;
  getClobMarketInfo(conditionId: string): Promise<MarketDetails>;
  getTrades(params?: { maker_address?: string }, onlyFirstPage?: boolean): Promise<Trade[]>;
  createAndPostMarketOrder(
    order: UserMarketOrderV2,
    options: { tickSize: TickSize; negRisk: boolean },
    orderType: OrderType.FOK | OrderType.FAK,
  ): Promise<OrderResponse>;
  cancelOrder(payload: { orderID: string }): Promise<CancelResult>;
  cancelAll(): Promise<CancelResult>;
}

interface CancelResult {
  canceled?: string[];
  not_canceled?: Record<string, string>;
}

interface RelayerClientLike {
  execute(
    txns: Transaction[],
    metadata?: string,
  ): Promise<RelayerTransactionResponse>;
  pollUntilState?(
    transactionId: string,
    states: string[],
    failState?: string,
    maxPolls?: number,
    pollFrequency?: number,
  ): Promise<RelayerTransaction | undefined>;
}

interface AuthState {
  creds: ApiKeyCreds;
  client: ClobVenueClient;
  walletAddress: string | null;
  proxyWallet: string | null;
}

interface AccountObservation {
  state: LiveAccountState;
  allowanceStatus: LiveCheckStatus;
  legalOrderMinUsd: number | null;
}

interface ProviderHealthState {
  lastSuccessfulAccountRefreshAtMs: number | null;
  lastAccountRefreshError: string | null;
  lastDiscoveryError: string | null;
  lastDiscoveryFallbackUsedAtMs: number | null;
}

interface ProviderDeps {
  fetchImpl: (
    input: URL | RequestInfo,
    init?: RequestInit,
  ) => Promise<Response>;
  nowMs: () => number;
  createClobClient: (
    config: SidecarConfig,
    creds?: ApiKeyCreds,
  ) => ClobVenueClient;
  createRelayerClient: (config: SidecarConfig) => RelayerClientLike;
  createUserStreamMonitor: (
    config: SidecarConfig,
    nowMs: () => number,
    logger: SidecarLogger,
  ) => UserStreamMonitor;
}

interface SidecarLogger {
  (level: LogLevel, event: string, fields?: Record<string, unknown>): void;
}

interface UserStreamSocketLike {
  readyState: number;
  send(data: string): void;
  close(code?: number, data?: string): void;
  terminate?(): void;
  on(event: "open", listener: () => void): this;
  on(event: "message", listener: (data: RawData) => void): this;
  on(event: "error", listener: (error: Error) => void): this;
  on(event: "close", listener: (code: number, reason: Buffer) => void): this;
}

interface WsUserStreamMonitorDeps {
  now: () => number;
  createSocket: (url: string) => UserStreamSocketLike;
  setTimeoutFn: typeof setTimeout;
  clearTimeoutFn: typeof clearTimeout;
  setIntervalFn: typeof setInterval;
  clearIntervalFn: typeof clearInterval;
  random: () => number;
  log: SidecarLogger;
  connectTimeoutMs: number;
  stableGraceMs: number;
  reconnectBaseMs: number;
  reconnectMaxMs: number;
}

interface ConnectionAttempt {
  id: number;
  revision: number;
  markets: string[];
  socket: UserStreamSocketLike;
  connectTimer: NodeJS.Timeout | null;
  readyTimer: NodeJS.Timeout | null;
  heartbeatTimer: NodeJS.Timeout | null;
  abort?: (error: unknown) => void;
}

export class ProviderStageError extends Error {
  constructor(
    public readonly stage: SidecarErrorStage,
    message: string,
  ) {
    super(`${stage}: ${message}`);
    this.name = "ProviderStageError";
  }
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

function signatureTypeFromConfig(value: number): SignatureTypeV2 {
  switch (value) {
    case SignatureTypeV2.EOA:
      return SignatureTypeV2.EOA;
    case SignatureTypeV2.POLY_PROXY:
      return SignatureTypeV2.POLY_PROXY;
    case SignatureTypeV2.POLY_GNOSIS_SAFE:
      return SignatureTypeV2.POLY_GNOSIS_SAFE;
    case SignatureTypeV2.POLY_1271:
      return SignatureTypeV2.POLY_1271;
    default:
      throw new Error(`unsupported POLYMARKET_SIGNATURE_TYPE ${value}`);
  }
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

function collateralUnitsToUsd(value: number): number {
  return value / COLLATERAL_DECIMALS;
}

function maxAllowanceUsd(balanceAllowance: BalanceAllowanceView): number | null {
  const view = balanceAllowance;
  const directAllowance = parseNumber((view as { allowance?: unknown }).allowance);
  if (directAllowance != null) {
    return collateralUnitsToUsd(directAllowance);
  }
  const allowanceEntries = Object.values(view.allowances ?? {})
    .map((entry) => parseNumber(entry))
    .filter((entry): entry is number => entry != null);
  if (allowanceEntries.length === 0) {
    return null;
  }
  return collateralUnitsToUsd(Math.max(...allowanceEntries));
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
    .map((market): ActiveMarket | null => {
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
        tokens: [],
        feeDetails: null,
        negRisk: null,
        metadataSource: "gamma",
        metadataError: null,
      } satisfies ActiveMarket;
    })
    .filter((market): market is ActiveMarket => market !== null);
}

function normalizeClobMarketInfo(
  market: ActiveMarket,
  info: MarketDetails,
): ActiveMarket {
  const raw = info as MarketDetails & {
    mos?: unknown;
    fd?: { r?: unknown; e?: unknown; to?: unknown };
    mbf?: unknown;
    tbf?: unknown;
  };
  const tokens = Array.isArray(raw.t)
    ? raw.t
        .filter((token): token is NonNullable<(typeof raw.t)[number]> => token != null)
        .map((token) => ({
          tokenId: token.t,
          outcome: token.o,
        }))
    : [];
  const feeDetails = raw.fd
    ? {
        rate: parseNumber(raw.fd.r),
        exponent: parseNumber(raw.fd.e),
        takerOnly: typeof raw.fd.to === "boolean" ? raw.fd.to : null,
        makerBaseFee: parseNumber(raw.mbf),
        takerBaseFee: parseNumber(raw.tbf),
      }
    : null;
  return {
    ...market,
    minOrderSize: parseNumber(raw.mos) ?? market.minOrderSize,
    tickSize: parseNumber(raw.mts) ?? market.tickSize,
    tokens,
    feeDetails,
    negRisk: typeof raw.nr === "boolean" ? raw.nr : market.negRisk,
    metadataSource: "clob_v2",
    metadataError: null,
  };
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

function orderTypeFromRequest(value: string): OrderType.FOK | OrderType.FAK | null {
  const normalized = value.trim().toUpperCase();
  if (normalized === OrderType.FOK) return OrderType.FOK;
  if (normalized === OrderType.FAK) return OrderType.FAK;
  return null;
}

function sideFromRequest(value: string): Side | null {
  const normalized = value.trim().toUpperCase();
  if (normalized === Side.BUY) return Side.BUY;
  if (normalized === Side.SELL) return Side.SELL;
  return null;
}

function isPositiveFinite(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isTickAligned(price: number, tickSize: number): boolean {
  if (!Number.isFinite(price) || !Number.isFinite(tickSize) || tickSize <= 0) {
    return false;
  }
  const units = Math.round(price / tickSize);
  return Math.abs(price - units * tickSize) < 1e-9;
}

function clobTickSize(tickSize: number): TickSize {
  const normalized = tickSize.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
  if (
    normalized === "0.1" ||
    normalized === "0.01" ||
    normalized === "0.001" ||
    normalized === "0.0001"
  ) {
    return normalized;
  }
  throw new ProviderStageError(
    "order_submission",
    `Unsupported CLOB V2 tick size ${tickSize}.`,
  );
}

function safeDetails(value: Record<string, unknown>): string {
  return JSON.stringify(value);
}

function safeNumber(value: unknown): number | null {
  const parsed = parseNumber(value);
  return parsed == null || !Number.isFinite(parsed) ? null : parsed;
}

function safeString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value : null;
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    const text = safeString(value);
    if (text) return text;
  }
  return null;
}

function makerOrderIds(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((entry) => {
      if (entry === null || typeof entry !== "object") return null;
      return firstString((entry as Record<string, unknown>).order_id);
    })
    .filter((entry): entry is string => entry !== null);
}

function activityEventFromRecord(
  source: "user_stream" | "clob_trades",
  timestampMs: number,
  record: Record<string, unknown>,
): LiveActivityEvent {
  const eventType = firstString(
    record.event_type,
    record.type,
    record.status,
    "unknown",
  ) as string;
  return {
    timestamp_ms: timestampMs,
    source,
    event_type: eventType,
    market_id: firstString(record.market, record.market_id, record.condition_id),
    order_id: firstString(record.order_id, record.orderID, record.id),
    trade_id: firstString(record.trade_id, record.tradeID, record.id),
    asset_id: firstString(record.asset_id, record.asset, record.token_id),
    side: firstString(record.side),
    price: safeNumber(record.price),
    size: safeNumber(record.size),
    status: firstString(record.status),
    details_json: safeDetails({
      outcome: firstString(record.outcome),
      liquidity_side: firstString(record.trader_side, record.liquidity_side),
      transaction_hash: firstString(record.transaction_hash, record.tx_hash),
      match_time: firstString(record.match_time),
      matchtime: firstString(record.matchtime),
      last_update: firstString(record.last_update),
      timestamp: firstString(record.timestamp),
      type: firstString(record.type),
      original_size: firstString(record.original_size),
      size_matched: firstString(record.size_matched),
      matched_amount: firstString(record.matched_amount),
      taker_order_id: firstString(record.taker_order_id),
      maker_order_ids: makerOrderIds(record.maker_orders),
    }),
  };
}

function userStreamActivityEvent(
  message: string,
  timestampMs: number,
): LiveActivityEvent | null {
  try {
    const parsed = JSON.parse(message) as unknown;
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      return null;
    }
    return activityEventFromRecord(
      "user_stream",
      timestampMs,
      parsed as Record<string, unknown>,
    );
  } catch {
    return null;
  }
}

function tradeActivityEvent(trade: Trade): LiveActivityEvent {
  const matchTime = safeNumber(trade.match_time);
  const timestampMs = matchTime != null ? Math.round(matchTime * 1000) : nowMs();
  return activityEventFromRecord("clob_trades", timestampMs, trade as unknown as Record<string, unknown>);
}

function normalizeAcceptedSize(
  side: Side,
  response: Partial<OrderResponse>,
): number | null {
  const raw =
    side === Side.BUY
      ? safeNumber(response.takingAmount)
      : safeNumber(response.makingAmount);
  return raw == null ? null : collateralUnitsToUsd(raw);
}

function normalizeOrderStatus(response: Partial<OrderResponse>): string {
  if (typeof response.status === "string" && response.status.trim().length > 0) {
    return response.status;
  }
  return response.success === true ? "submitted" : "rejected";
}

function redactOrderResponse(response: Partial<OrderResponse>): Record<string, unknown> {
  return {
    success: response.success,
    errorMsg: response.errorMsg,
    orderID: response.orderID,
    transactionsHashes: response.transactionsHashes,
    status: response.status,
    takingAmount: response.takingAmount,
    makingAmount: response.makingAmount,
  };
}

function redeemableConditionIds(positions: PositionResponse[]): {
  conditionIds: string[];
  unsupported: PositionResponse[];
  missingConditionId: PositionResponse[];
} {
  const redeemable = positions.filter((position) => position.redeemable === true);
  const unsupported = redeemable.filter(
    (position) => position.negativeRisk === true,
  );
  const missingConditionId = redeemable.filter((position) => {
    const conditionId = position.conditionId ?? position.condition_id;
    return typeof conditionId !== "string" || conditionId.trim().length === 0;
  });
  const conditionIds = dedupeStrings(
    redeemable
      .filter((position) => position.negativeRisk !== true)
      .map((position) => position.conditionId ?? position.condition_id ?? "")
      .filter((conditionId) => conditionId.trim().length > 0),
  );
  return { conditionIds, unsupported, missingConditionId };
}

const ctfRedeemAbi = [
  {
    name: "redeemPositions",
    type: "function",
    stateMutability: "nonpayable",
    inputs: [
      { name: "collateralToken", type: "address" },
      { name: "parentCollectionId", type: "bytes32" },
      { name: "conditionId", type: "bytes32" },
      { name: "indexSets", type: "uint256[]" },
    ],
    outputs: [],
  },
] as const;

function createRedeemTransaction(conditionId: string): Transaction {
  return {
    to: CTF_ADDRESS,
    data: encodeFunctionData({
      abi: ctfRedeemAbi,
      functionName: "redeemPositions",
      args: [
        PUSD_COLLATERAL_ADDRESS as Hex,
        ZERO_BYTES32,
        conditionId as Hex,
        [1n, 2n],
      ],
    }),
    value: "0",
  };
}

function hasBuilderRelayerCredentials(config: SidecarConfig): boolean {
  return Boolean(
    config.builderApiKey &&
      config.builderSecret &&
      config.builderPassphrase,
  );
}

function configuredApiKeyCreds(config: SidecarConfig): ApiKeyCreds | null {
  const configuredCount = [
    config.apiKey,
    config.apiSecret,
    config.apiPassphrase,
  ].filter(Boolean).length;
  if (configuredCount > 0 && configuredCount < 3) {
    throw new Error(
      "incomplete CLOB L2 credentials; set POLYMARKET_API_KEY, POLYMARKET_API_SECRET, and POLYMARKET_API_PASSPHRASE together",
    );
  }
  if (!config.apiKey || !config.apiSecret || !config.apiPassphrase) {
    return null;
  }
  return {
    key: config.apiKey,
    secret: config.apiSecret,
    passphrase: config.apiPassphrase,
  };
}

function compactErrorBody(body: string): string {
  return body.replace(/\s+/g, " ").slice(0, 240);
}

function normalizeApiKeyCreds(value: unknown): ApiKeyCreds {
  if (!value || typeof value !== "object") {
    throw new Error("CLOB API key response is not an object");
  }
  const record = value as Record<string, unknown>;
  const key = typeof record.key === "string"
    ? record.key
    : typeof record.apiKey === "string"
      ? record.apiKey
      : null;
  const secret = typeof record.secret === "string" ? record.secret : null;
  const passphrase = typeof record.passphrase === "string"
    ? record.passphrase
    : null;
  if (!key || !secret || !passphrase) {
    throw new Error("CLOB API key response is missing key, secret, or passphrase");
  }
  return { key, secret, passphrase };
}

function postHttp11Json(
  target: URL,
  headers: Record<string, string | number | boolean>,
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const request = https.request(
      {
        protocol: target.protocol,
        hostname: target.hostname,
        port: target.port || undefined,
        path: `${target.pathname}${target.search}`,
        method: "POST",
        headers: {
          ...headers,
          "User-Agent": "@polymarket/clob-client",
          Accept: "*/*",
          "Content-Type": "application/json",
          "Content-Length": "0",
        },
        agent: new https.Agent({ keepAlive: false }),
      },
      (response) => {
        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          body += chunk;
        });
        response.on("end", () => {
          if (response.statusCode !== 200) {
            reject(
              new Error(
                `CLOB HTTP/1.1 API key create failed status=${response.statusCode ?? "unknown"} body=${compactErrorBody(body)}`,
              ),
            );
            return;
          }
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(new Error(`CLOB API key response JSON parse failed: ${stringError(error)}`));
          }
        });
      },
    );
    request.on("error", reject);
    request.end();
  });
}

function getHttp11Json(
  target: URL,
  headers: Record<string, string | number | boolean>,
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const request = https.request(
      {
        protocol: target.protocol,
        hostname: target.hostname,
        port: target.port || undefined,
        path: `${target.pathname}${target.search}`,
        method: "GET",
        headers: {
          ...headers,
          "User-Agent": "@polymarket/clob-client",
          Accept: "*/*",
          "Content-Type": "application/json",
        },
        agent: new https.Agent({ keepAlive: false }),
      },
      (response) => {
        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          body += chunk;
        });
        response.on("end", () => {
          if (response.statusCode !== 200) {
            reject(
              new Error(
                `CLOB HTTP/1.1 GET failed status=${response.statusCode ?? "unknown"} path=${target.pathname} body=${compactErrorBody(body)}`,
              ),
            );
            return;
          }
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(new Error(`CLOB HTTP/1.1 JSON parse failed: ${stringError(error)}`));
          }
        });
      },
    );
    request.on("error", reject);
    request.end();
  });
}

async function signedClobGetHttp11(
  config: SidecarConfig,
  signer: Wallet,
  creds: ApiKeyCreds,
  endpoint: string,
  params: Record<string, string | number | boolean | undefined>,
  timestamp: number,
): Promise<unknown> {
  const host = new URL(config.clobHost);
  if (host.protocol !== "https:") {
    throw new Error("CLOB HTTP/1.1 authenticated GET requires an https host");
  }
  const target = new URL(endpoint, host);
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) {
      target.searchParams.set(key, String(value));
    }
  }
  const headers = await createL2Headers(
    signer,
    creds,
    { method: "GET", requestPath: endpoint },
    timestamp,
  );
  return getHttp11Json(target, headers);
}

function requireApiCreds(creds: ApiKeyCreds | undefined): ApiKeyCreds {
  if (!creds) {
    throw new Error("CLOB API credentials are required for authenticated readonly access");
  }
  return creds;
}

function balanceAllowanceView(value: unknown): BalanceAllowanceView {
  if (!value || typeof value !== "object") {
    throw new Error("CLOB balance/allowance response is not an object");
  }
  const record = value as Record<string, unknown>;
  if (typeof record.balance !== "string") {
    throw new Error("CLOB balance/allowance response is missing balance");
  }
  return record as unknown as BalanceAllowanceView;
}

function paginatedData<T>(value: unknown): { data: T[]; nextCursor: string } {
  if (!value || typeof value !== "object") {
    throw new Error("CLOB paginated response is not an object");
  }
  const record = value as Record<string, unknown>;
  const data = Array.isArray(record.data) ? (record.data as T[]) : [];
  const nextCursor = typeof record.next_cursor === "string"
    ? record.next_cursor
    : CLOB_END_CURSOR;
  return { data, nextCursor };
}

async function createApiKeyOverHttp11(
  config: SidecarConfig,
  signer: Wallet,
  nonce?: number,
): Promise<ApiKeyCreds> {
  const host = new URL(config.clobHost);
  if (host.protocol !== "https:") {
    throw new Error("CLOB HTTP/1.1 API key fallback requires an https host");
  }
  const headers = await createL1Headers(signer, CHAIN_ID, nonce);
  const response = await postHttp11Json(
    new URL("/auth/api-key", host),
    headers,
  );
  return normalizeApiKeyCreds(response);
}

function relayerTxTypeFromConfig(config: SidecarConfig): typeof RelayerTxType.PROXY | typeof RelayerTxType.SAFE {
  return config.signatureType === SignatureTypeV2.POLY_GNOSIS_SAFE
    ? RelayerTxType.SAFE
    : RelayerTxType.PROXY;
}

function stringError(error: unknown): string {
  if (error instanceof ProviderStageError) {
    return error.message;
  }
  const status = errorStatus(error);
  const message = error instanceof Error ? error.message : String(error);
  if (status === 425 && !message.includes("425")) {
    return `${message} (HTTP 425 matching engine restart)`;
  }
  return message;
}

function errorStatus(error: unknown): number | null {
  if (error === null || typeof error !== "object") {
    return null;
  }
  const record = error as {
    status?: unknown;
    statusCode?: unknown;
    response?: { status?: unknown };
  };
  if (typeof record.status === "number") {
    return record.status;
  }
  if (typeof record.statusCode === "number") {
    return record.statusCode;
  }
  if (typeof record.response?.status === "number") {
    return record.response.status;
  }
  return null;
}

function isVenueRestart(error: unknown): boolean {
  return errorStatus(error) === 425 || stringError(error).includes("HTTP 425");
}

function isAuthLikeError(error: unknown): boolean {
  const message = stringError(error).toLowerCase();
  return (
    message.includes("401") ||
    message.includes("403") ||
    message.includes("forbidden") ||
    message.includes("unauthorized") ||
    message.includes("api key") ||
    message.includes("auth")
  );
}

function arraysEqual(left: string[], right: string[]): boolean {
  if (left.length !== right.length) return false;
  return left.every((value, index) => value === right[index]);
}

function computeReconnectDelayMs(
  failures: number,
  baseMs: number,
  maxMs: number,
  random: () => number,
): number {
  const exponent = Math.max(failures - 1, 0);
  const capped = Math.min(baseMs * 2 ** exponent, maxMs);
  const jitter = 0.8 + random() * 0.4;
  return Math.max(1, Math.round(capped * jitter));
}

function safeCloseSocket(socket: UserStreamSocketLike, log: SidecarLogger): void {
  try {
    if (socket.readyState === WebSocket.CONNECTING) {
      if (typeof socket.terminate === "function") {
        socket.terminate();
      }
      return;
    }
    if (
      socket.readyState === WebSocket.OPEN ||
      socket.readyState === WebSocket.CLOSING
    ) {
      socket.close();
    }
  } catch (error) {
    log("warn", "user_stream_socket_close_failed", {
      error: stringError(error),
    });
  }
}

function stageError(stage: SidecarErrorStage, error: unknown): ProviderStageError {
  if (error instanceof ProviderStageError) {
    return error;
  }
  return new ProviderStageError(stage, stringError(error));
}

async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  stage: SidecarErrorStage,
  action: string,
): Promise<T> {
  let timer: NodeJS.Timeout | null = null;
  try {
    return await new Promise<T>((resolve, reject) => {
      timer = setTimeout(() => {
        reject(
          new ProviderStageError(
            stage,
            `${action} timed out after ${timeoutMs}ms`,
          ),
        );
      }, timeoutMs);
      promise.then(resolve, reject);
    });
  } catch (error) {
    throw stageError(stage, error);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export class WsUserStreamMonitor implements UserStreamMonitor {
  private desired:
    | { auth: ApiKeyCreds; markets: string[]; revision: number }
    | null = null;
  private readonly state: UserStreamSnapshot = {
    status: "failed",
    lifecycle: "idle",
    lastConnectedAtMs: null,
    lastEventAtMs: null,
    lastError: null,
    lastDisconnectedAtMs: null,
    lastDisconnectReason: null,
    consecutiveFailures: 0,
    subscribedMarkets: [],
    recentEvents: [],
  };
  private currentAttempt: ConnectionAttempt | null = null;
  private connectPromise: Promise<void> | null = null;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private desiredRevision = 0;
  private nextAttemptId = 0;
  private closed = false;

  constructor(private readonly deps: WsUserStreamMonitorDeps) {}

  async ensureConnected(auth: ApiKeyCreds, markets: string[]): Promise<void> {
    const desiredMarkets = dedupeStrings(markets).sort();
    if (desiredMarkets.length === 0) {
      throw new Error("no market ids available for the authenticated user stream");
    }
    if (this.closed) {
      throw new Error("authenticated user stream monitor is closed");
    }

    this.desired = {
      auth,
      markets: desiredMarkets,
      revision: ++this.desiredRevision,
    };

    if (
      this.state.status === "ok" &&
      arraysEqual(this.state.subscribedMarkets, desiredMarkets)
    ) {
      return;
    }

    if (!this.connectPromise) {
      this.connectPromise = this.ensureDesiredConnection().finally(() => {
        this.connectPromise = null;
      });
    }
    return this.connectPromise;
  }

  snapshot(): UserStreamSnapshot {
    return {
      ...this.state,
      subscribedMarkets: [...this.state.subscribedMarkets],
      recentEvents: [...this.state.recentEvents],
    };
  }

  close(): void {
    this.closed = true;
    this.desired = null;
    this.clearReconnectTimer();
    if (this.currentAttempt) {
      this.disposeAttempt(this.currentAttempt, "monitor_closed");
    }
    this.currentAttempt = null;
    this.state.status = "failed";
    this.state.lifecycle = "closed";
    this.state.lastError = "monitor closed";
  }

  private async ensureDesiredConnection(): Promise<void> {
    while (!this.closed) {
      const desired = this.desired;
      if (!desired) return;
      if (
        this.state.status === "ok" &&
        arraysEqual(this.state.subscribedMarkets, desired.markets)
      ) {
        return;
      }
      await this.openConnection(desired);
      if (
        !this.desired ||
        arraysEqual(this.state.subscribedMarkets, this.desired.markets)
      ) {
        return;
      }
      if (this.currentAttempt) {
        this.disposeAttempt(this.currentAttempt, "subscription_updated");
      }
    }
  }

  private async openConnection(desired: {
    auth: ApiKeyCreds;
    markets: string[];
    revision: number;
  }): Promise<void> {
    this.clearReconnectTimer();
    if (this.currentAttempt) {
      this.disposeAttempt(this.currentAttempt, "subscription_updated");
    }
    const reconnecting =
      this.state.lifecycle === "reconnecting" || this.state.consecutiveFailures > 0;
    this.state.lifecycle = reconnecting ? "reconnecting" : "connecting";
    this.deps.log("info", "user_stream_connect_start", {
      markets: desired.markets,
      reconnecting,
      failures: this.state.consecutiveFailures,
    });
    const socket = this.deps.createSocket(USER_STREAM_URL);
    const attempt: ConnectionAttempt = {
      id: ++this.nextAttemptId,
      revision: desired.revision,
      markets: desired.markets,
      socket,
      connectTimer: null,
      readyTimer: null,
      heartbeatTimer: null,
    };
    this.currentAttempt = attempt;

    await new Promise<void>((resolve, reject) => {
      let settled = false;

      const completeFailure = (error: unknown): void => {
        if (settled) return;
        settled = true;
        const failure = stageError("user_stream_connect", error);
        this.failAttempt(attempt, failure);
        reject(failure);
      };
      attempt.abort = completeFailure;

      const completeSuccess = (): void => {
        if (settled) return;
        settled = true;
        if (!this.isActiveAttempt(attempt)) {
          safeCloseSocket(socket, this.deps.log);
          resolve();
          return;
        }
        this.clearAttemptStartupTimers(attempt);
        this.state.status = "ok";
        this.state.lifecycle = "connected";
        this.state.lastConnectedAtMs = this.deps.now();
        this.state.lastError = null;
        this.state.subscribedMarkets = [...attempt.markets];
        this.state.consecutiveFailures = 0;
        this.deps.log("info", "user_stream_connect_ok", {
          markets: attempt.markets,
        });
        resolve();
      };

      attempt.connectTimer = this.deps.setTimeoutFn(() => {
        completeFailure(
          new Error("timed out connecting to the authenticated user stream"),
        );
      }, this.deps.connectTimeoutMs);

      socket.on("open", () => {
        if (!this.isActiveAttempt(attempt)) {
          safeCloseSocket(socket, this.deps.log);
          return;
        }
        try {
          socket.send(
            JSON.stringify({
              auth: {
                apiKey: desired.auth.key,
                secret: desired.auth.secret,
                passphrase: desired.auth.passphrase,
              },
              markets: desired.markets,
              type: "user",
            }),
          );
          this.startHeartbeat(attempt);
        } catch (error) {
          completeFailure(error);
          return;
        }
        attempt.readyTimer = this.deps.setTimeoutFn(() => {
          completeSuccess();
        }, this.deps.stableGraceMs);
      });

      socket.on("message", (data: RawData) => {
        if (!this.isActiveAttempt(attempt)) {
          return;
        }
        this.state.lastEventAtMs = this.deps.now();
        const message = data.toString();
        const activity = userStreamActivityEvent(message, this.state.lastEventAtMs);
        if (activity) {
          this.state.recentEvents.push(activity);
          this.state.recentEvents = this.state.recentEvents.slice(-200);
        }
        try {
          const parsed = JSON.parse(message) as Record<string, unknown>;
          if (
            !settled &&
            ((parsed.type === "error" && typeof parsed.error === "string") ||
              (parsed.event_type === "error" && typeof parsed.error === "string"))
          ) {
            completeFailure(new Error(parsed.error as string));
            return;
          }
        } catch {
          void 0;
        }
        if (!settled) {
          completeSuccess();
        }
      });

      socket.on("error", (error: Error) => {
        if (!this.isActiveAttempt(attempt)) {
          return;
        }
        if (!settled) {
          completeFailure(error);
          return;
        }
        this.markDisconnected(error.message);
      });

      socket.on("close", (code: number, reason: Buffer) => {
        if (!this.isActiveAttempt(attempt)) {
          return;
        }
        const message = `user stream closed (${code}): ${reason.toString()}`;
        if (!settled) {
          completeFailure(new Error(message));
          return;
        }
        this.markDisconnected(message);
      });
    });
  }

  private failAttempt(attempt: ConnectionAttempt, error: ProviderStageError): void {
    if (!this.isActiveAttempt(attempt)) {
      return;
    }
    this.clearAttemptTimers(attempt);
    this.state.status = "failed";
    this.state.lastError = error.message;
    this.state.lifecycle = this.closed ? "closed" : "reconnecting";
    this.state.lastDisconnectedAtMs = this.deps.now();
    this.state.lastDisconnectReason = error.message;
    this.state.consecutiveFailures += 1;
    this.deps.log("warn", "user_stream_connect_failed", {
      error: error.message,
      failures: this.state.consecutiveFailures,
      markets: attempt.markets,
    });
    this.currentAttempt = null;
    safeCloseSocket(attempt.socket, this.deps.log);
    this.scheduleReconnect();
  }

  private markDisconnected(message: string): void {
    const attempt = this.currentAttempt;
    if (attempt) {
      this.clearAttemptTimers(attempt);
    }
    this.state.status = "failed";
    this.state.lastError = message;
    this.state.lifecycle = this.closed ? "closed" : "reconnecting";
    this.state.lastDisconnectedAtMs = this.deps.now();
    this.state.lastDisconnectReason = message;
    this.state.consecutiveFailures += 1;
    this.deps.log("warn", "user_stream_disconnected", {
      reason: message,
      failures: this.state.consecutiveFailures,
    });
    this.currentAttempt = null;
    this.scheduleReconnect();
  }

  private startHeartbeat(attempt: ConnectionAttempt): void {
    if (attempt.heartbeatTimer) {
      return;
    }
    attempt.heartbeatTimer = this.deps.setIntervalFn(() => {
      if (!this.isActiveAttempt(attempt) || attempt.socket.readyState !== WebSocket.OPEN) {
        return;
      }
      try {
        attempt.socket.send("PING");
      } catch (error) {
        this.markDisconnected(stageError("user_stream_connect", error).message);
        safeCloseSocket(attempt.socket, this.deps.log);
      }
    }, USER_STREAM_PING_INTERVAL_MS);
  }

  private scheduleReconnect(): void {
    if (this.closed || !this.desired || this.reconnectTimer) {
      return;
    }
    const delayMs = computeReconnectDelayMs(
      this.state.consecutiveFailures,
      this.deps.reconnectBaseMs,
      this.deps.reconnectMaxMs,
      this.deps.random,
    );
    this.deps.log("info", "user_stream_reconnect_scheduled", {
      delay_ms: delayMs,
      markets: this.desired.markets,
    });
    this.reconnectTimer = this.deps.setTimeoutFn(() => {
      this.reconnectTimer = null;
      if (this.closed || !this.desired) {
        return;
      }
      if (!this.connectPromise) {
        this.connectPromise = this.ensureDesiredConnection().finally(() => {
          this.connectPromise = null;
        });
      }
      void this.connectPromise.catch(() => undefined);
    }, delayMs);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      this.deps.clearTimeoutFn(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private clearAttemptTimers(attempt: ConnectionAttempt): void {
    this.clearAttemptStartupTimers(attempt);
    if (attempt.heartbeatTimer) {
      this.deps.clearIntervalFn(attempt.heartbeatTimer);
      attempt.heartbeatTimer = null;
    }
  }

  private clearAttemptStartupTimers(attempt: ConnectionAttempt): void {
    if (attempt.connectTimer) {
      this.deps.clearTimeoutFn(attempt.connectTimer);
      attempt.connectTimer = null;
    }
    if (attempt.readyTimer) {
      this.deps.clearTimeoutFn(attempt.readyTimer);
      attempt.readyTimer = null;
    }
  }

  private disposeAttempt(attempt: ConnectionAttempt, reason: string): void {
    if (!this.isActiveAttempt(attempt)) {
      return;
    }
    this.clearAttemptTimers(attempt);
    this.currentAttempt = null;
    attempt.abort?.(new Error(`connection cancelled: ${reason}`));
    this.deps.log("info", "user_stream_connect_cancelled", {
      reason,
      markets: attempt.markets,
    });
    safeCloseSocket(attempt.socket, this.deps.log);
  }

  private isActiveAttempt(attempt: ConnectionAttempt): boolean {
    return this.currentAttempt?.id === attempt.id;
  }
}

function defaultProviderDeps(): ProviderDeps {
  const logger: SidecarLogger = (level, event, fields = {}) => {
    logEvent(level, event, fields);
  };
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
      const configuredCreds = configuredApiKeyCreds(config);
      const effectiveCreds = creds ?? configuredCreds ?? undefined;
      const client = new ClobClient({
        host: config.clobHost,
        chain: CHAIN_ID,
        signer,
        creds: effectiveCreds,
        signatureType: signatureTypeFromConfig(config.signatureType),
        funderAddress: walletAddress(config) ?? undefined,
        useServerTime: true,
        retryOnError: true,
        throwOnError: true,
      });
      return {
        createOrDeriveApiKey: async () => {
          if (effectiveCreds) {
            return effectiveCreds;
          }
          try {
            return await client.deriveApiKey();
          } catch (deriveError) {
            try {
              return await client.createApiKey();
            } catch (createError) {
              try {
                return await createApiKeyOverHttp11(config, signer);
              } catch (fallbackError) {
                throw new Error(
                  `derive API key failed: ${compactErrorBody(stringError(deriveError))}; create API key failed: ${compactErrorBody(stringError(createError))}; HTTP/1.1 create fallback failed: ${compactErrorBody(stringError(fallbackError))}`,
                );
              }
            }
          }
        },
        getServerTime: async () => client.getServerTime(),
        getBalanceAllowance: async () =>
          balanceAllowanceView(
            await signedClobGetHttp11(
              config,
              signer,
              requireApiCreds(effectiveCreds),
              "/balance-allowance",
              {
                asset_type: AssetType.COLLATERAL,
                signature_type: signatureTypeFromConfig(config.signatureType),
              },
              await client.getServerTime(),
            ),
          ),
        getOpenOrders: async () => {
          const orders: OpenOrder[] = [];
          let cursor = CLOB_INITIAL_CURSOR;
          while (cursor !== CLOB_END_CURSOR) {
            const page = paginatedData<OpenOrder>(
              await signedClobGetHttp11(
                config,
                signer,
                requireApiCreds(effectiveCreds),
                "/data/orders",
                { next_cursor: cursor },
                await client.getServerTime(),
              ),
            );
            orders.push(...page.data);
            cursor = page.nextCursor;
          }
          return orders;
        },
        getClobMarketInfo: async (conditionId) =>
          client.getClobMarketInfo(conditionId),
        getTrades: async (params, onlyFirstPage) => {
          const trades: Trade[] = [];
          let cursor = CLOB_INITIAL_CURSOR;
          while (cursor !== CLOB_END_CURSOR) {
            const page = paginatedData<Trade>(
              await signedClobGetHttp11(
                config,
                signer,
                requireApiCreds(effectiveCreds),
                "/data/trades",
                { ...params, next_cursor: cursor },
                await client.getServerTime(),
              ),
            );
            trades.push(...page.data);
            if (onlyFirstPage) {
              break;
            }
            cursor = page.nextCursor;
          }
          return trades;
        },
        createAndPostMarketOrder: async (order, options, orderType) =>
          client.createAndPostMarketOrder(order, options, orderType),
        cancelOrder: async (payload) => client.cancelOrder(payload),
        cancelAll: async () => client.cancelAll(),
      };
    },
    createRelayerClient: (config) => {
      if (!config.privateKey) {
        throw new Error(
          "POLYMARKET_PRIVATE_KEY is required for Polymarket relayer redemption",
        );
      }
      const signer = new Wallet(config.privateKey);
      const builderConfig = hasBuilderRelayerCredentials(config)
        ? new BuilderConfig({
            localBuilderCreds: {
              key: config.builderApiKey as string,
              secret: config.builderSecret as string,
              passphrase: config.builderPassphrase as string,
            },
          })
        : undefined;
      return new RelayClient(
        config.relayerHost,
        CHAIN_ID,
        signer,
        builderConfig,
        relayerTxTypeFromConfig(config),
      );
    },
    createUserStreamMonitor: (config, clock, log) =>
      new WsUserStreamMonitor({
        now: clock,
        createSocket: (url) => new WebSocket(url),
        setTimeoutFn: setTimeout,
        clearTimeoutFn: clearTimeout,
        setIntervalFn: setInterval,
        clearIntervalFn: clearInterval,
        random: Math.random,
        log,
        connectTimeoutMs: config.userStreamConnectTimeoutMs,
        stableGraceMs: config.userStreamStableGraceMs,
        reconnectBaseMs: config.userStreamReconnectBaseMs,
        reconnectMaxMs: config.userStreamReconnectMaxMs,
      }),
  };
}

function withPolymarketHttpHeaders(init: RequestInit): RequestInit {
  const headers = new Headers(init.headers);
  if (!headers.has("Accept")) {
    headers.set("Accept", "application/json");
  }
  if (!headers.has("User-Agent")) {
    headers.set("User-Agent", POLYMARKET_HTTP_USER_AGENT);
  }
  return { ...init, headers };
}

export interface SidecarProvider {
  health(): Promise<SidecarHealthResponse>;
  preflight(request: LivePreflightRequest): Promise<LivePreflightResponse>;
  accountState(): Promise<LiveAccountState>;
  activity(): Promise<LiveActivityResponse>;
  submitOrderIntent(
    request: LiveOrderIntentRequest,
  ): Promise<LiveOrderIntentResponse>;
  cancelOrder(orderId: string): Promise<LiveCancellationResponse>;
  cancelAll(): Promise<LiveCancellationResponse>;
  redeemAll(): Promise<LiveRedemptionResponse>;
  close(): Promise<void> | void;
}

export class StubSidecarProvider implements SidecarProvider {
  constructor(private readonly config: SidecarConfig) {}

  async health(): Promise<SidecarHealthResponse> {
    return {
      ok: true,
      ready: false,
      readiness_status: "failed",
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
      last_successful_account_refresh_at_ms: null,
      last_account_refresh_error: "stub provider",
      last_user_stream_disconnect_at_ms: null,
      last_user_stream_disconnect_reason: null,
      consecutive_user_stream_failures: 0,
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
        clob_contract_version: "v2",
        collateral_token: "pUSD",
        collateral_decimals: 6,
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
        clob_contract_version: "v2",
        collateral_token: "pUSD",
        collateral_decimals: 6,
        account_state_not_verified: true,
        relayer_api_key_present: Boolean(this.config.relayerApiKey),
      }),
    };
  }

  async activity(): Promise<LiveActivityResponse> {
    return {
      timestamp_ms: nowMs(),
      user_stream_status: "failed",
      user_stream_events: [],
      clob_trades: [],
      details_json: JSON.stringify({
        provider: "stub",
        reason: "activity not available from stub provider",
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

  close(): void {
    void 0;
  }
}

export class PolymarketReadonlyProvider implements SidecarProvider {
  private readonly deps: ProviderDeps;
  private readonly userStream: UserStreamMonitor;
  private readonly log: SidecarLogger;
  private authState: AuthState | null = null;
  private authStatePromise: Promise<AuthState> | null = null;
  private lastGammaApiUrl: string = DEFAULT_GAMMA_API_URL;
  private lastDiscovery: ActiveMarket[] = [];
  private readonly submittedOrderResponses = new Map<
    string,
    LiveOrderIntentResponse
  >();
  private readonly healthState: ProviderHealthState = {
    lastSuccessfulAccountRefreshAtMs: null,
    lastAccountRefreshError: null,
    lastDiscoveryError: null,
    lastDiscoveryFallbackUsedAtMs: null,
  };

  constructor(
    private readonly config: SidecarConfig,
    deps: Partial<ProviderDeps> = {},
  ) {
    const defaults = defaultProviderDeps();
    this.deps = {
      fetchImpl: deps.fetchImpl ?? defaults.fetchImpl,
      nowMs: deps.nowMs ?? defaults.nowMs,
      createClobClient: deps.createClobClient ?? defaults.createClobClient,
      createRelayerClient: deps.createRelayerClient ?? defaults.createRelayerClient,
      createUserStreamMonitor:
        deps.createUserStreamMonitor ?? defaults.createUserStreamMonitor,
    };
    this.log = (level, event, fields = {}) => {
      logEvent(level, event, fields);
    };
    this.userStream = this.deps.createUserStreamMonitor(
      this.config,
      this.deps.nowMs,
      this.log,
    );
  }

  async health(): Promise<SidecarHealthResponse> {
    const stream = this.userStream.snapshot();
    const readiness = this.readinessStatus(stream);
    return {
      ok: true,
      ready: readiness === "ready",
      readiness_status: readiness,
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
      last_successful_account_refresh_at_ms:
        this.healthState.lastSuccessfulAccountRefreshAtMs,
      last_account_refresh_error: this.healthState.lastAccountRefreshError,
      last_user_stream_disconnect_at_ms: stream.lastDisconnectedAtMs,
      last_user_stream_disconnect_reason: stream.lastDisconnectReason,
      consecutive_user_stream_failures: stream.consecutiveFailures,
    };
  }

  async preflight(request: LivePreflightRequest): Promise<LivePreflightResponse> {
    this.lastGammaApiUrl = request.gamma_api_url;

    const errors: string[] = [];
    const geoblock = await this.fetchGeoblock().catch((error: unknown) => {
      const failure = stageError("geoblock_check", error);
      errors.push(`Geoblock check failed: ${failure.message}`);
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
      const failure = stageError("auth_bootstrap", error);
      errors.push(`Authentication failed: ${failure.message}`);
    }

    if (authState) {
      try {
        const serverTimeMs = normalizeServerTimeMs(
          await this.runClobCall("clock_check", async () =>
            authState.client.getServerTime(),
          ),
        );
        const driftMs = Math.abs(serverTimeMs - this.deps.nowMs());
        clockStatus = checkStatus(driftMs <= this.config.clockDriftMaxMs);
        if (clockStatus === "failed") {
          errors.push(
            `Clock drift ${driftMs}ms exceeds POLYMARKET_CLOCK_DRIFT_MAX_MS ${this.config.clockDriftMaxMs}ms.`,
          );
        }
      } catch (error) {
        const failure = stageError("clock_check", error);
        errors.push(`Clock check failed: ${failure.message}`);
      }

      try {
        discovery = await this.discoverActiveMarkets(request.gamma_api_url);
        discovery = await this.enrichActiveMarkets(
          authState.client,
          discovery,
        );
        this.lastDiscovery = discovery.markets;
        legalOrderMinUsd = discovery.legalOrderMinUsd;
        if (discovery.markets.length === 0) {
          errors.push("No active BTC 5-minute markets were discovered from Gamma.");
        }
        if (discovery.degraded && discovery.error) {
          errors.push(`Active-market discovery degraded: ${discovery.error}`);
        }
      } catch (error) {
        const failure = stageError("market_discovery", error);
        errors.push(`Active-market discovery failed: ${failure.message}`);
      }

      if (discovery && discovery.markets.length > 0) {
        try {
          await this.userStream.ensureConnected(
            authState.creds,
            discovery.markets.map((market) => market.conditionId),
          );
          userStreamStatus = "ok";
        } catch (error) {
          const failure = stageError("user_stream_connect", error);
          errors.push(`Authenticated user stream failed: ${failure.message}`);
        }
      }

      try {
        const account = await this.observeAccountState(
          authState,
          discovery?.markets ?? [],
          request.gamma_api_url,
          discovery ?? undefined,
        );
        allowanceStatus = account.allowanceStatus;
        availableCashUsd = account.state.cash_available;
        legalOrderMinUsd = legalOrderMinUsd ?? account.legalOrderMinUsd;
      } catch (error) {
        const failure = error instanceof ProviderStageError ? error : null;
        errors.push(
          `Account state could not be read: ${failure?.message ?? stringError(error)}`,
        );
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
        clob_contract_version: "v2",
        collateral_token: "pUSD",
        collateral_decimals: 6,
        strategy_readiness: request.strategy_readiness,
        relayer_api_key_present: Boolean(this.config.relayerApiKey),
        geoblock,
        active_markets: discovery?.markets ?? this.lastDiscovery,
        discovery_degraded: discovery?.degraded ?? false,
        discovery_error: discovery?.error ?? this.healthState.lastDiscoveryError,
        wallet_address_source: this.config.funder ? "funder" : "proxy_wallet",
        last_user_stream_connected_at_ms: stream.lastConnectedAtMs,
        last_user_stream_event_at_ms: stream.lastEventAtMs,
        last_user_stream_error: stream.lastError,
        last_user_stream_disconnect_at_ms: stream.lastDisconnectedAtMs,
        last_user_stream_disconnect_reason: stream.lastDisconnectReason,
        last_successful_account_refresh_at_ms:
          this.healthState.lastSuccessfulAccountRefreshAtMs,
        last_account_refresh_error: this.healthState.lastAccountRefreshError,
      }),
      errors,
    };
  }

  async accountState(): Promise<LiveAccountState> {
    try {
      const authState = await this.getAuthState();
      const gammaApiUrl = this.lastGammaApiUrl || DEFAULT_GAMMA_API_URL;
      let discovery: ActiveMarketDiscovery;
      try {
        discovery = await this.discoverActiveMarkets(gammaApiUrl);
        discovery = await this.enrichActiveMarkets(authState.client, discovery);
      } catch (error) {
        if (this.lastDiscovery.length === 0) {
          throw error;
        }
        const failure = stageError("market_discovery", error);
        this.healthState.lastDiscoveryError = failure.message;
        this.healthState.lastDiscoveryFallbackUsedAtMs = this.deps.nowMs();
        this.log("warn", "market_discovery_fallback", {
          error: failure.message,
          cached_market_count: this.lastDiscovery.length,
        });
        discovery = {
          markets: this.lastDiscovery,
          legalOrderMinUsd: legalOrderMinUsd(this.lastDiscovery),
          degraded: true,
          error: failure.message,
          usedFallback: true,
        };
      }
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
        discovery,
      );
      return observation.state;
    } catch (error) {
      const failure = error instanceof ProviderStageError
        ? error
        : new ProviderStageError("balance_allowance", stringError(error));
      this.recordAccountRefreshFailure(failure);
      throw failure;
    }
  }

  async activity(): Promise<LiveActivityResponse> {
    const timestampMs = this.deps.nowMs();
    const stream = this.userStream.snapshot();
    try {
      const authState = await this.getAuthState();
      const makerAddress = authState.proxyWallet ?? authState.walletAddress ?? undefined;
      const trades = await this.runClobCall("activity", async () =>
        authState.client.getTrades(
          makerAddress ? { maker_address: makerAddress } : undefined,
          true,
        ),
      );
      return {
        timestamp_ms: timestampMs,
        user_stream_status: stream.status,
        user_stream_events: stream.recentEvents,
        clob_trades: trades.map(tradeActivityEvent),
        details_json: safeDetails({
          provider: "polymarket",
          clob_contract_version: "v2",
          collateral_token: "pUSD",
          user_stream_lifecycle: stream.lifecycle,
          user_stream_event_count: stream.recentEvents.length,
          clob_trade_count: trades.length,
          maker_address: makerAddress ?? null,
        }),
      };
    } catch (error) {
      const failure = stageError("activity", error);
      return {
        timestamp_ms: timestampMs,
        user_stream_status: stream.status,
        user_stream_events: stream.recentEvents,
        clob_trades: [],
        details_json: safeDetails({
          provider: "polymarket",
          action: "activity",
          error: failure.message,
        }),
      };
    }
  }

  async submitOrderIntent(
    request: LiveOrderIntentRequest,
  ): Promise<LiveOrderIntentResponse> {
    const prior = this.submittedOrderResponses.get(request.client_order_id);
    if (prior) {
      return {
        ...prior,
        status_reason: prior.status_reason ?? "Duplicate client order id.",
        details_json: safeDetails({
          provider: "polymarket",
          duplicate_client_order_id: true,
          original: JSON.parse(prior.details_json ?? "{}") as unknown,
        }),
      };
    }

    const receivedAtMs = this.deps.nowMs();
    const orderType = orderTypeFromRequest(request.order_type);
    const side = sideFromRequest(request.side);
    if (!orderType || !side) {
      return this.rejectedOrder(request, "validation_failed", "Only FOK/FAK BUY/SELL orders are supported.", {
        received_at_ms: receivedAtMs,
      });
    }
    if (!isPositiveFinite(request.limit_price) || request.limit_price >= 1) {
      return this.rejectedOrder(request, "validation_failed", "limit_price must be between 0 and 1.", {
        received_at_ms: receivedAtMs,
      });
    }
    const orderAmount =
      side === Side.BUY ? request.amount_usd : request.size;
    if (!isPositiveFinite(orderAmount)) {
      return this.rejectedOrder(
        request,
        "validation_failed",
        side === Side.BUY
          ? "BUY market orders require positive amount_usd."
          : "SELL market orders require positive size.",
        { received_at_ms: receivedAtMs },
      );
    }

    try {
      const authState = await this.getAuthState();
      const discovery = await this.enrichActiveMarkets(
        authState.client,
        await this.discoverActiveMarkets(this.lastGammaApiUrl || DEFAULT_GAMMA_API_URL),
      );
      this.lastDiscovery = discovery.markets;
      const market = discovery.markets.find(
        (candidate) => candidate.conditionId === request.market_id,
      );
      const validationFailure = this.validateOrderMarket(request, market, orderAmount);
      if (validationFailure) {
        return this.rejectedOrder(
          request,
          "validation_failed",
          validationFailure,
          {
            received_at_ms: receivedAtMs,
            active_markets: discovery.markets,
          },
        );
      }
      const orderMarket = market as ActiveMarket;

      const account = await this.observeAccountState(
        authState,
        discovery.markets,
        this.lastGammaApiUrl || DEFAULT_GAMMA_API_URL,
        discovery,
      );
      const accountFailure = await this.validateOrderAccount(
        authState,
        request,
        side,
        orderAmount,
        account.state,
      );
      if (accountFailure) {
        return this.rejectedOrder(
          request,
          "account_unavailable",
          accountFailure,
          {
            received_at_ms: receivedAtMs,
            cash_available: account.state.cash_available,
            allowance_available: account.state.allowance_available,
          },
        );
      }

      const submitStartedAtMs = this.deps.nowMs();
      const response = await this.runClobCall("order_submission", async () =>
        authState.client.createAndPostMarketOrder(
          {
            tokenID: request.token_id,
            side,
            amount: orderAmount,
            price: request.limit_price,
            orderType,
          },
          {
            tickSize: clobTickSize(orderMarket.tickSize as number),
            negRisk: orderMarket.negRisk === true,
          },
          orderType,
        ),
      );
      const acceptedSize = normalizeAcceptedSize(side, response);
      const status = normalizeOrderStatus(response);
      const result: LiveOrderIntentResponse = {
        ok: response.success === true,
        venue_order_id:
          typeof response.orderID === "string" && response.orderID.length > 0
            ? response.orderID
            : null,
        client_order_id: request.client_order_id,
        status,
        status_reason:
          response.success === true ? null : response.errorMsg || "CLOB rejected order.",
        accepted_size: acceptedSize,
        details_json: safeDetails({
          provider: "polymarket",
          clob_contract_version: "v2",
          collateral_token: "pUSD",
          client_order_id: request.client_order_id,
          condition_id: request.market_id,
          token_id: request.token_id,
          side,
          order_type: orderType,
          amount: orderAmount,
          amount_semantics: side === Side.BUY ? "usd" : "shares",
          limit_price: request.limit_price,
          tick_size: orderMarket.tickSize ?? null,
          min_order_size: orderMarket.minOrderSize ?? null,
          fee_details: orderMarket.feeDetails ?? null,
          neg_risk: orderMarket.negRisk ?? null,
          received_at_ms: receivedAtMs,
          submit_started_at_ms: submitStartedAtMs,
          submit_finished_at_ms: this.deps.nowMs(),
          raw_response: redactOrderResponse(response),
        }),
      };
      this.submittedOrderResponses.set(request.client_order_id, result);
      this.log(result.ok ? "info" : "warn", "live_order_submit_result", {
        client_order_id: request.client_order_id,
        venue_order_id: result.venue_order_id,
        status: result.status,
        ok: result.ok,
      });
      return result;
    } catch (error) {
      const failure = stageError("order_submission", error);
      const status = isVenueRestart(failure) ? "venue_restart" : "unknown_submission";
      const result: LiveOrderIntentResponse = {
        ok: false,
        venue_order_id: null,
        client_order_id: request.client_order_id,
        status,
        status_reason: failure.message,
        accepted_size: null,
        details_json: safeDetails({
          provider: "polymarket",
          client_order_id: request.client_order_id,
          condition_id: request.market_id,
          token_id: request.token_id,
          received_at_ms: receivedAtMs,
          failed_at_ms: this.deps.nowMs(),
          dangerous_unknown_submission: status === "unknown_submission",
          error: failure.message,
        }),
      };
      this.submittedOrderResponses.set(request.client_order_id, result);
      this.log("error", "live_order_submit_failed", {
        client_order_id: request.client_order_id,
        status,
        error: failure.message,
      });
      return result;
    }
  }

  async cancelOrder(orderId: string): Promise<LiveCancellationResponse> {
    try {
      const authState = await this.getAuthState();
      const response = await this.runClobCall("cancel_order", async () =>
        authState.client.cancelOrder({ orderID: orderId }),
      );
      return this.normalizeCancelResponse("cancel_order", response);
    } catch (error) {
      const failure = stageError("cancel_order", error);
      return {
        ok: false,
        cancelled: 0,
        details_json: safeDetails({
          provider: "polymarket",
          action: "cancel_order",
          order_id: orderId,
          venue_restart: isVenueRestart(failure),
          error: failure.message,
        }),
      };
    }
  }

  async cancelAll(): Promise<LiveCancellationResponse> {
    try {
      const authState = await this.getAuthState();
      const response = await this.runClobCall("cancel_all", async () =>
        authState.client.cancelAll(),
      );
      return this.normalizeCancelResponse("cancel_all", response);
    } catch (error) {
      const failure = stageError("cancel_all", error);
      return {
        ok: false,
        cancelled: 0,
        details_json: safeDetails({
          provider: "polymarket",
          action: "cancel_all",
          venue_restart: isVenueRestart(failure),
          error: failure.message,
        }),
      };
    }
  }

  async redeemAll(): Promise<LiveRedemptionResponse> {
    try {
      const authState = await this.getAuthState();
      if (!authState.walletAddress) {
        throw new ProviderStageError("redemption", "wallet address is missing");
      }
      const positions = await this.fetchPositions(
        authState.walletAddress,
        this.lastGammaApiUrl || DEFAULT_GAMMA_API_URL,
      );
      const { conditionIds, unsupported, missingConditionId } =
        redeemableConditionIds(positions);
      if (unsupported.length > 0 || missingConditionId.length > 0) {
        return {
          ok: false,
          submitted: 0,
          details_json: safeDetails({
            provider: "polymarket",
            action: "redeem_all",
            unsupported_negative_risk_count: unsupported.length,
            missing_condition_id_count: missingConditionId.length,
            reason:
              "Redeem-all is fail-closed until every redeemable position has a supported condition id.",
          }),
        };
      }
      if (conditionIds.length === 0) {
        return {
          ok: true,
          submitted: 0,
          details_json: safeDetails({
            provider: "polymarket",
            action: "redeem_all",
            redeemable_condition_ids: [],
          }),
        };
      }
      if (!hasBuilderRelayerCredentials(this.config)) {
        return {
          ok: false,
          submitted: 0,
          details_json: safeDetails({
            provider: "polymarket",
            action: "redeem_all",
            redeemable_condition_ids: conditionIds,
            relayer_api_key_present: Boolean(this.config.relayerApiKey),
            relayer_api_key_address_present: Boolean(
              this.config.relayerApiKeyAddress,
            ),
            reason:
              "Current installed relayer SDK exposes builder-credential auth for execute(); configure POLYMARKET_BUILDER_API_KEY, POLYMARKET_BUILDER_SECRET, and POLYMARKET_BUILDER_PASSPHRASE or update the SDK before redeeming.",
          }),
        };
      }

      const relayer = this.deps.createRelayerClient(this.config);
      const transactions = conditionIds.map(createRedeemTransaction);
      const response = await withTimeout(
        relayer.execute(transactions, "buba-paint redeem positions"),
        this.config.sdkTimeoutMs,
        "redemption",
        "submit redemption",
      );
      const pollTimeoutMs =
        this.config.redemptionPollIntervalMs * this.config.redemptionMaxPolls +
        this.config.sdkTimeoutMs;
      const terminal = await withTimeout(
        relayer.pollUntilState
          ? relayer.pollUntilState(
              response.transactionID,
              [
                RelayerTransactionState.STATE_MINED,
                RelayerTransactionState.STATE_CONFIRMED,
              ],
              RelayerTransactionState.STATE_FAILED,
              this.config.redemptionMaxPolls,
              this.config.redemptionPollIntervalMs,
            )
          : response.wait(),
        pollTimeoutMs,
        "redemption",
        "poll redemption",
      );
      const terminalState = terminal?.state ?? response.state;
      const ok =
        terminalState === RelayerTransactionState.STATE_MINED ||
        terminalState === RelayerTransactionState.STATE_CONFIRMED;
      return {
        ok,
        submitted: ok ? conditionIds.length : 0,
        details_json: safeDetails({
          provider: "polymarket",
          action: "redeem_all",
          redeemable_condition_ids: conditionIds,
          transaction_id: response.transactionID,
          initial_state: response.state,
          terminal_state: terminalState,
          transaction_hash:
            terminal?.transactionHash || response.transactionHash || response.hash || null,
          submitted_count: conditionIds.length,
          collateral_token: PUSD_COLLATERAL_ADDRESS,
          ctf_address: CTF_ADDRESS,
          relayer_tx_type: relayerTxTypeFromConfig(this.config),
          proceeds_counted_spendable: false,
          requires_account_refresh: ok,
        }),
      };
    } catch (error) {
      const failure = stageError("redemption", error);
      return {
        ok: false,
        submitted: 0,
        details_json: safeDetails({
          provider: "polymarket",
          action: "redeem_all",
          error: failure.message,
        }),
      };
    }
  }

  private rejectedOrder(
    request: LiveOrderIntentRequest,
    status: string,
    reason: string,
    details: Record<string, unknown>,
  ): LiveOrderIntentResponse {
    return {
      ok: false,
      venue_order_id: null,
      client_order_id: request.client_order_id,
      status,
      status_reason: reason,
      accepted_size: null,
      details_json: safeDetails({
        provider: "polymarket",
        client_order_id: request.client_order_id,
        condition_id: request.market_id,
        token_id: request.token_id,
        reason,
        ...details,
      }),
    };
  }

  private validateOrderMarket(
    request: LiveOrderIntentRequest,
    market: ActiveMarket | undefined,
    orderAmount: number,
  ): string | null {
    if (!market) {
      return "Order market was not found in current active BTC 5-minute discovery.";
    }
    if (market.acceptingOrders !== true) {
      return "Order market is not accepting orders.";
    }
    if (market.metadataSource !== "clob_v2" || market.metadataError) {
      return "CLOB V2 market metadata is unavailable.";
    }
    if (!isPositiveFinite(market.tickSize)) {
      return "CLOB V2 market tick size is unavailable.";
    }
    if (!isTickAligned(request.limit_price, market.tickSize)) {
      return `limit_price is not aligned to tick size ${market.tickSize}.`;
    }
    if (!isPositiveFinite(market.minOrderSize)) {
      return "CLOB V2 market min order size is unavailable.";
    }
    if (orderAmount < market.minOrderSize) {
      return `Order amount ${orderAmount} is below market min size ${market.minOrderSize}.`;
    }
    if (
      market.tokens.length > 0 &&
      !market.tokens.some((token) => token.tokenId === request.token_id)
    ) {
      return "Order token_id is not one of the active market tokens.";
    }
    return null;
  }

  private async validateOrderAccount(
    authState: AuthState,
    request: LiveOrderIntentRequest,
    side: Side,
    orderAmount: number,
    account: LiveAccountState,
  ): Promise<string | null> {
    if (side === Side.BUY) {
      if (account.cash_available < orderAmount) {
        return `Available pUSD cash ${account.cash_available.toFixed(2)} is below order amount ${orderAmount.toFixed(2)}.`;
      }
      if (account.allowance_available == null) {
        return "Allowance state is unavailable.";
      }
      if (account.allowance_available < orderAmount) {
        return `Available pUSD allowance ${account.allowance_available.toFixed(2)} is below order amount ${orderAmount.toFixed(2)}.`;
      }
      return null;
    }

    if (!authState.walletAddress) {
      return "Wallet address is unavailable for SELL inventory validation.";
    }
    const positions = await this.fetchPositions(
      authState.walletAddress,
      this.lastGammaApiUrl || DEFAULT_GAMMA_API_URL,
    );
    const tokenPosition = positions.find(
      (position) => position.asset === request.token_id,
    );
    const tokenSize = parseNumber(tokenPosition?.size) ?? 0;
    if (tokenSize < orderAmount) {
      return `Token inventory ${tokenSize.toFixed(4)} is below SELL size ${orderAmount.toFixed(4)}.`;
    }
    return null;
  }

  private normalizeCancelResponse(
    action: "cancel_order" | "cancel_all",
    response: CancelResult,
  ): LiveCancellationResponse {
    const canceled = Array.isArray(response.canceled) ? response.canceled : [];
    const notCanceled =
      response.not_canceled && typeof response.not_canceled === "object"
        ? response.not_canceled
        : {};
    return {
      ok: Object.keys(notCanceled).length === 0,
      cancelled: canceled.length,
      details_json: safeDetails({
        provider: "polymarket",
        action,
        canceled,
        not_canceled: notCanceled,
      }),
    };
  }

  close(): void {
    this.userStream.close();
  }

  private readinessStatus(stream: UserStreamSnapshot): ReadinessStatus {
    if (!hasAuthCredentials(this.config)) {
      return "failed";
    }
    if (this.healthState.lastSuccessfulAccountRefreshAtMs == null) {
      return "failed";
    }
    if (
      stream.status === "ok" &&
      this.healthState.lastAccountRefreshError == null &&
      this.healthState.lastDiscoveryError == null
    ) {
      return "ready";
    }
    if (this.healthState.lastDiscoveryError && this.lastDiscovery.length === 0) {
      return "failed";
    }
    return "degraded";
  }

  private async getAuthState(): Promise<AuthState> {
    if (this.authState) {
      return this.authState;
    }
    if (this.authStatePromise) {
      return this.authStatePromise;
    }
    this.authStatePromise = (async () => {
      if (!hasAuthCredentials(this.config)) {
        throw new ProviderStageError(
          "auth_bootstrap",
          "Missing POLY_PROXY credentials. Set POLYMARKET_PRIVATE_KEY, POLYMARKET_PROXY_WALLET, and POLYMARKET_FUNDER.",
        );
      }
      this.log("info", "auth_bootstrap_start");
      const bootstrapClient = this.deps.createClobClient(this.config);
      const creds = await this.runClobCall("auth_bootstrap", async () =>
        bootstrapClient.createOrDeriveApiKey(),
      );
      const state = {
        creds,
        client: this.deps.createClobClient(this.config, creds),
        walletAddress: walletAddress(this.config),
        proxyWallet: this.config.proxyWallet,
      } satisfies AuthState;
      this.authState = state;
      this.log("info", "auth_bootstrap_ok", {
        wallet_address: state.walletAddress,
        proxy_wallet: state.proxyWallet,
      });
      return state;
    })()
      .catch((error) => {
        this.invalidateAuthState("auth bootstrap failed");
        this.log("error", "auth_bootstrap_failed", {
          error: stringError(error),
        });
        throw stageError("auth_bootstrap", error);
      })
      .finally(() => {
        this.authStatePromise = null;
      });
    return this.authStatePromise;
  }

  private invalidateAuthState(reason: string): void {
    this.authState = null;
    this.authStatePromise = null;
    this.log("warn", "auth_state_invalidated", { reason });
  }

  private async fetchJson<T>(
    url: string | URL,
    stage: SidecarErrorStage,
  ): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.config.httpTimeoutMs);
    try {
      const response = await withTimeout(
        this.deps.fetchImpl(
          url,
          withPolymarketHttpHeaders({
            signal: controller.signal,
          }),
        ),
        this.config.httpTimeoutMs,
        stage,
        "request",
      );
      if (!response.ok) {
        const suffix =
          response.status === 425 ? " (matching engine restart)" : "";
        throw new ProviderStageError(
          stage,
          `endpoint returned ${response.status}${suffix}`,
        );
      }
      return (await response.json()) as T;
    } catch (error) {
      const message =
        error instanceof Error && error.name === "AbortError"
          ? `request timed out after ${this.config.httpTimeoutMs}ms`
          : stringError(error);
      throw new ProviderStageError(stage, message);
    } finally {
      clearTimeout(timer);
    }
  }

  private async fetchGeoblock(): Promise<GeoblockResponse> {
    return this.fetchJson<GeoblockResponse>(
      this.config.geoblockUrl,
      "geoblock_check",
    );
  }

  private async discoverActiveMarkets(
    gammaApiUrl: string,
  ): Promise<ActiveMarketDiscovery> {
    const markets: ActiveMarket[] = [];
    const errors: string[] = [];

    for (const slug of currentSlotSlugs(this.deps.nowMs())) {
      const url = `${gammaApiUrl}/events/slug/${slug}`;
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), this.config.httpTimeoutMs);
      try {
        const response = await withTimeout(
          this.deps.fetchImpl(
            url,
            withPolymarketHttpHeaders({
              signal: controller.signal,
            }),
          ),
          this.config.httpTimeoutMs,
          "market_discovery",
          `gamma discovery for ${slug}`,
        );
        if (response.status === 404) {
          continue;
        }
          if (!response.ok) {
            throw new ProviderStageError(
              "market_discovery",
              `gamma discovery returned ${response.status} for ${slug}`,
            );
          }
          markets.push(...parseActiveMarkets(await response.json()));
        } finally {
          clearTimeout(timer);
        }
      } catch (error) {
        errors.push(stageError("market_discovery", error).message);
      }
    }

    const deduped = dedupeStrings(markets.map((market) => market.conditionId)).map(
      (conditionId) =>
        markets.find((market) => market.conditionId === conditionId) as ActiveMarket,
    );

    if (errors.length === 0) {
      this.healthState.lastDiscoveryError = null;
      return {
        markets: deduped,
        legalOrderMinUsd: legalOrderMinUsd(deduped),
        degraded: false,
        error: null,
        usedFallback: false,
      };
    }

    const message = errors.join("; ");
    this.healthState.lastDiscoveryError = message;
    if (deduped.length > 0) {
      this.healthState.lastDiscoveryFallbackUsedAtMs = this.deps.nowMs();
      this.log("warn", "market_discovery_degraded", {
        error: message,
        discovered_market_count: deduped.length,
      });
      return {
        markets: deduped,
        legalOrderMinUsd: legalOrderMinUsd(deduped),
        degraded: true,
        error: message,
        usedFallback: false,
      };
    }

    this.log("error", "market_discovery_failed", {
      error: message,
    });
    throw new ProviderStageError("market_discovery", message);
  }

  private async enrichActiveMarkets(
    client: ClobVenueClient,
    discovery: ActiveMarketDiscovery,
  ): Promise<ActiveMarketDiscovery> {
    if (discovery.markets.length === 0) {
      return discovery;
    }

    const errors: string[] = [];
    const enriched = await Promise.all(
      discovery.markets.map(async (market) => {
        try {
          const info = await this.runClobCall("market_metadata", async () =>
            client.getClobMarketInfo(market.conditionId),
          );
          return normalizeClobMarketInfo(market, info);
        } catch (error) {
          const failure = stageError("market_metadata", error);
          errors.push(failure.message);
          return {
            ...market,
            metadataError: failure.message,
          };
        }
      }),
    );

    if (errors.length === 0) {
      this.healthState.lastDiscoveryError = discovery.error;
      return {
        ...discovery,
        markets: enriched,
        legalOrderMinUsd: legalOrderMinUsd(enriched),
      };
    }

    const message = errors.join("; ");
    this.healthState.lastDiscoveryError = discovery.error
      ? `${discovery.error}; ${message}`
      : message;
    this.log("warn", "market_metadata_degraded", {
      error: message,
      discovered_market_count: enriched.length,
      clob_v2_metadata_count: enriched.filter(
        (market) => market.metadataSource === "clob_v2",
      ).length,
    });
    return {
      ...discovery,
      markets: enriched,
      legalOrderMinUsd: legalOrderMinUsd(enriched),
      degraded: true,
      error: this.healthState.lastDiscoveryError,
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
    const parsed = await this.fetchJson<unknown>(url, "positions");
    return Array.isArray(parsed) ? (parsed as PositionResponse[]) : [];
  }

  private async runClobCall<T>(
    stage: SidecarErrorStage,
    fn: () => Promise<T>,
  ): Promise<T> {
    try {
      return await withTimeout(fn(), this.config.sdkTimeoutMs, stage, stage);
    } catch (error) {
      if (isAuthLikeError(error)) {
        this.invalidateAuthState(`${stage} failed with an auth-like error`);
      }
      throw stageError(stage, error);
    }
  }

  private async observeAccountState(
    authState: AuthState,
    markets: ActiveMarket[],
    gammaApiUrl: string,
    discovery?: ActiveMarketDiscovery,
  ): Promise<AccountObservation> {
    if (!authState.walletAddress) {
      const failure = new ProviderStageError(
        "balance_allowance",
        "wallet address is missing",
      );
      this.recordAccountRefreshFailure(failure);
      throw failure;
    }

    try {
      const [balanceAllowance, openOrders, positions] = await Promise.all([
        this.runClobCall("balance_allowance", async () =>
          authState.client.getBalanceAllowance(),
        ),
        this.runClobCall("open_orders", async () => authState.client.getOpenOrders()),
        this.fetchPositions(authState.walletAddress, gammaApiUrl),
      ]);

      const rawBalance =
        collateralUnitsToUsd(parseNumber(balanceAllowance.balance) ?? 0);
      const rawAllowance = maxAllowanceUsd(balanceAllowance);
      const cashReservedForOrders = sumReservedBuyCash(openOrders);
      const inventoryMarkValue = positions
        .filter((position) => position.redeemable !== true)
        .reduce(
          (total, position) => total + (parseNumber(position.currentValue) ?? 0),
          0,
        );
      const redeemableValue = positions
        .filter((position) => position.redeemable === true)
        .reduce(
          (total, position) => total + (parseNumber(position.currentValue) ?? 0),
          0,
        );
      const cashAvailable = Math.max(rawBalance - cashReservedForOrders, 0);
      const cappedAllowanceUsd =
        rawAllowance == null ? null : Math.max(Math.min(rawAllowance, rawBalance), 0);
      const allowanceAvailable =
        cappedAllowanceUsd == null
          ? null
          : Math.max(cappedAllowanceUsd - cashReservedForOrders, 0);
      const stream = this.userStream.snapshot();
      const legalMinUsd = legalOrderMinUsd(markets);
      const refreshAtMs = this.deps.nowMs();
      const accountState: LiveAccountState = {
        timestamp_ms: refreshAtMs,
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
          clob_contract_version: "v2",
          collateral_token: "pUSD",
          collateral_decimals: 6,
          relayer_api_key_present: Boolean(this.config.relayerApiKey),
          user_stream_status: stream.status,
          last_user_stream_connected_at_ms: stream.lastConnectedAtMs,
          last_user_stream_event_at_ms: stream.lastEventAtMs,
          last_user_stream_disconnect_at_ms: stream.lastDisconnectedAtMs,
          last_user_stream_disconnect_reason: stream.lastDisconnectReason,
          last_successful_account_refresh_at_ms: refreshAtMs,
          account_refresh_error: null,
          discovery_degraded: discovery?.degraded ?? false,
          discovery_error: discovery?.error ?? this.healthState.lastDiscoveryError,
          legal_order_min_usd: legalMinUsd,
          open_order_count: openOrders.length,
          open_buy_order_count: openOrders.filter((order) => order.side === "BUY")
            .length,
          open_sell_order_count: openOrders.filter((order) => order.side === "SELL")
            .length,
          position_count: positions.length,
          redeemable_position_count: positions.filter(
            (position) => position.redeemable === true,
          ).length,
          observed_balance_usd: rawBalance,
          observed_allowance_usd: cappedAllowanceUsd,
          observed_allowance_approval_usd: rawAllowance,
          active_markets: markets,
        }),
      };
      this.recordAccountRefreshSuccess(refreshAtMs);
      return {
        state: accountState,
        allowanceStatus: checkStatus(rawAllowance != null),
        legalOrderMinUsd: legalMinUsd,
      };
    } catch (error) {
      const failure =
        error instanceof ProviderStageError
          ? error
          : new ProviderStageError("balance_allowance", stringError(error));
      this.recordAccountRefreshFailure(failure);
      this.log("warn", "account_refresh_failed", {
        error: failure.message,
      });
      throw failure;
    }
  }

  private recordAccountRefreshSuccess(timestampMs: number): void {
    this.healthState.lastSuccessfulAccountRefreshAtMs = timestampMs;
    this.healthState.lastAccountRefreshError = null;
  }

  private recordAccountRefreshFailure(error: ProviderStageError): void {
    this.healthState.lastAccountRefreshError = error.message;
  }
}

export function createDefaultProvider(
  config: SidecarConfig,
): SidecarProvider {
  return new PolymarketReadonlyProvider(config);
}
