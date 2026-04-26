import { describe, expect, it } from "vitest";
import { loadConfig } from "../config.js";

describe("loadConfig", () => {
  it("uses proxy-wallet defaults for magic-link accounts", () => {
    const config = loadConfig({
      POLYMARKET_PRIVATE_KEY: "pk",
      POLYMARKET_PROXY_WALLET: "0xproxy",
      POLYMARKET_RELAYER_API_KEY: "relayer",
    });

    expect(config.signatureType).toBe(1);
    expect(config.proxyWallet).toBe("0xproxy");
    expect(config.funder).toBe("0xproxy");
    expect(config.relayerApiKey).toBe("relayer");
    expect(config.httpTimeoutMs).toBe(5000);
    expect(config.sdkTimeoutMs).toBe(5000);
    expect(config.userStreamConnectTimeoutMs).toBe(5000);
    expect(config.userStreamStableGraceMs).toBe(1000);
    expect(config.userStreamReconnectBaseMs).toBe(1000);
    expect(config.userStreamReconnectMaxMs).toBe(30000);
  });

  it("ignores blank string overrides", () => {
    const config = loadConfig({
      POLYMARKET_PROXY_WALLET: "  ",
      POLYMARKET_FUNDER: "  ",
    });

    expect(config.proxyWallet).toBeNull();
    expect(config.funder).toBeNull();
  });

  it("loads timeout and reconnect overrides", () => {
    const config = loadConfig({
      POLYMARKET_HTTP_TIMEOUT_MS: "7000",
      POLYMARKET_SDK_TIMEOUT_MS: "8000",
      POLYMARKET_USER_STREAM_CONNECT_TIMEOUT_MS: "9000",
      POLYMARKET_USER_STREAM_STABLE_GRACE_MS: "1100",
      POLYMARKET_USER_STREAM_RECONNECT_BASE_MS: "1200",
      POLYMARKET_USER_STREAM_RECONNECT_MAX_MS: "25000",
    });

    expect(config.httpTimeoutMs).toBe(7000);
    expect(config.sdkTimeoutMs).toBe(8000);
    expect(config.userStreamConnectTimeoutMs).toBe(9000);
    expect(config.userStreamStableGraceMs).toBe(1100);
    expect(config.userStreamReconnectBaseMs).toBe(1200);
    expect(config.userStreamReconnectMaxMs).toBe(25000);
  });
});
