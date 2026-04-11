import { describe, expect, it } from "vitest";
import { StubSidecarProvider } from "../provider.js";
import { loadConfig } from "../config.js";

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

  it("returns explicit stub metadata for disabled order flow", async () => {
    const provider = new StubSidecarProvider(
      loadConfig({
        POLYMARKET_PROXY_WALLET: "0xproxy",
      }),
    );

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
    const cancel = await provider.cancelAll();
    const redeem = await provider.redeemAll();

    expect(order.status).toBe("not_implemented");
    expect(order.details_json).toContain("\"provider\":\"stub\"");
    expect(cancel.details_json).toContain("cancel not implemented");
    expect(redeem.details_json).toContain("redemption not implemented");
  });
});
