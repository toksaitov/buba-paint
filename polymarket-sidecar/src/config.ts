export interface SidecarConfig {
  port: number;
  host: string;
  geoblockUrl: string;
  clobHost: string;
  relayerHost: string;
  polygonRpcUrl: string;
  clockDriftMaxMs: number;
  expectedTakerFeeRate: number;
  maxOrderUsd: number;
  httpTimeoutMs: number;
  sdkTimeoutMs: number;
  userStreamConnectTimeoutMs: number;
  userStreamStableGraceMs: number;
  userStreamStalenessMs: number;
  userStreamReconnectBaseMs: number;
  userStreamReconnectMaxMs: number;
  redemptionPollIntervalMs: number;
  redemptionMaxPolls: number;
  signatureType: number;
  privateKey: string | null;
  proxyWallet: string | null;
  funder: string | null;
  apiKey: string | null;
  apiSecret: string | null;
  apiPassphrase: string | null;
  relayerApiKey: string | null;
  relayerApiKeyAddress: string | null;
  builderApiKey: string | null;
  builderSecret: string | null;
  builderPassphrase: string | null;
}

/// Parse one integer environment variable with a fallback.
function envInt(env: NodeJS.ProcessEnv, key: string, fallback: number): number {
  const raw = env[key];
  if (!raw) return fallback;
  const value = Number.parseInt(raw, 10);
  return Number.isFinite(value) ? value : fallback;
}

/// Parse one floating-point environment variable with a fallback.
function envFloat(env: NodeJS.ProcessEnv, key: string, fallback: number): number {
  const raw = env[key]?.trim();
  if (!raw) return fallback;
  const value = Number(raw);
  return Number.isFinite(value) ? value : fallback;
}

/// Read one string environment variable with an optional fallback.
function envStr(
  env: NodeJS.ProcessEnv,
  key: string,
  fallback: string | null = null,
): string | null {
  const value = env[key]?.trim();
  return value && value.length > 0 ? value : fallback;
}

/// Load the sidecar configuration from the provided environment.
export function loadConfig(env: NodeJS.ProcessEnv = process.env): SidecarConfig {
  const port = Number.parseInt(env.SIDECAR_PORT ?? "3210", 10);
  const proxyWallet = envStr(env, "POLYMARKET_PROXY_WALLET");
  return {
    port: Number.isFinite(port) ? port : 3210,
    host: env.SIDECAR_HOST ?? "127.0.0.1",
    geoblockUrl: env.POLYMARKET_GEOBLOCK_URL ?? "https://polymarket.com/api/geoblock",
    clobHost: env.POLYMARKET_CLOB_HOST ?? "https://clob.polymarket.com",
    relayerHost: env.POLYMARKET_RELAYER_HOST ?? "https://relayer-v2.polymarket.com",
    polygonRpcUrl: env.POLYMARKET_POLYGON_RPC_URL ?? "https://polygon-rpc.com",
    clockDriftMaxMs: envInt(env, "POLYMARKET_CLOCK_DRIFT_MAX_MS", 1500),
    expectedTakerFeeRate: envFloat(env, "POLYMARKET_EXPECTED_TAKER_FEE_RATE", 0.07),
    maxOrderUsd: envFloat(env, "SIDECAR_MAX_ORDER_USD", 0),
    httpTimeoutMs: envInt(env, "POLYMARKET_HTTP_TIMEOUT_MS", 5000),
    sdkTimeoutMs: envInt(env, "POLYMARKET_SDK_TIMEOUT_MS", 5000),
    userStreamConnectTimeoutMs: envInt(
      env,
      "POLYMARKET_USER_STREAM_CONNECT_TIMEOUT_MS",
      5000,
    ),
    userStreamStableGraceMs: envInt(
      env,
      "POLYMARKET_USER_STREAM_STABLE_GRACE_MS",
      1000,
    ),
    userStreamStalenessMs: envInt(
      env,
      "POLYMARKET_USER_STREAM_STALENESS_MS",
      45000,
    ),
    userStreamReconnectBaseMs: envInt(
      env,
      "POLYMARKET_USER_STREAM_RECONNECT_BASE_MS",
      1000,
    ),
    userStreamReconnectMaxMs: envInt(
      env,
      "POLYMARKET_USER_STREAM_RECONNECT_MAX_MS",
      30000,
    ),
    redemptionPollIntervalMs: envInt(
      env,
      "POLYMARKET_REDEMPTION_POLL_INTERVAL_MS",
      3000,
    ),
    redemptionMaxPolls: envInt(env, "POLYMARKET_REDEMPTION_MAX_POLLS", 20),
    signatureType: envInt(env, "POLYMARKET_SIGNATURE_TYPE", 1),
    privateKey: envStr(env, "POLYMARKET_PRIVATE_KEY"),
    proxyWallet,
    funder: envStr(env, "POLYMARKET_FUNDER", proxyWallet),
    apiKey: envStr(env, "POLYMARKET_API_KEY"),
    apiSecret: envStr(env, "POLYMARKET_API_SECRET"),
    apiPassphrase: envStr(env, "POLYMARKET_API_PASSPHRASE"),
    relayerApiKey: envStr(env, "POLYMARKET_RELAYER_API_KEY"),
    relayerApiKeyAddress: envStr(env, "POLYMARKET_RELAYER_API_KEY_ADDRESS"),
    builderApiKey: envStr(env, "POLYMARKET_BUILDER_API_KEY"),
    builderSecret: envStr(env, "POLYMARKET_BUILDER_SECRET"),
    builderPassphrase: envStr(env, "POLYMARKET_BUILDER_PASSPHRASE"),
  };
}
