export interface SidecarConfig {
  port: number;
  host: string;
  geoblockUrl: string;
  clobHost: string;
  relayerHost: string;
  clockDriftMaxMs: number;
  signatureType: number;
  privateKey: string | null;
  proxyWallet: string | null;
  funder: string | null;
  relayerApiKey: string | null;
}

/// Parse one integer environment variable with a fallback.
function envInt(env: NodeJS.ProcessEnv, key: string, fallback: number): number {
  const raw = env[key];
  if (!raw) return fallback;
  const value = Number.parseInt(raw, 10);
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
    relayerHost: env.POLYMARKET_RELAYER_HOST ?? "https://relayer.polymarket.com",
    clockDriftMaxMs: envInt(env, "POLYMARKET_CLOCK_DRIFT_MAX_MS", 1500),
    signatureType: envInt(env, "POLYMARKET_SIGNATURE_TYPE", 1),
    privateKey: envStr(env, "POLYMARKET_PRIVATE_KEY"),
    proxyWallet,
    funder: envStr(env, "POLYMARKET_FUNDER", proxyWallet),
    relayerApiKey: envStr(env, "POLYMARKET_RELAYER_API_KEY"),
  };
}
