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
  });

  it("ignores blank string overrides", () => {
    const config = loadConfig({
      POLYMARKET_PROXY_WALLET: "  ",
      POLYMARKET_FUNDER: "  ",
    });

    expect(config.proxyWallet).toBeNull();
    expect(config.funder).toBeNull();
  });
});
