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

export interface SidecarProvider {
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

function checkStatus(ok: boolean): "ok" | "failed" {
  return ok ? "ok" : "failed";
}

function walletAddress(config: SidecarConfig): string | null {
  return config.funder ?? config.proxyWallet;
}

export class StubSidecarProvider implements SidecarProvider {
  constructor(private readonly config: SidecarConfig) {}

  async preflight(request: LivePreflightRequest): Promise<LivePreflightResponse> {
    const authOk = Boolean(
      this.config.privateKey && this.config.proxyWallet && this.config.relayerApiKey,
    );
    const availableCashUsd = null;
    const legalOrderMinUsd = 5;
    const errors: string[] = [];
    if (!authOk) {
      errors.push(
        "Missing POLY_PROXY credentials. Set POLYMARKET_PRIVATE_KEY, POLYMARKET_PROXY_WALLET, and POLYMARKET_RELAYER_API_KEY.",
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
      status_reason: "Live order routing is intentionally disabled in the stub sidecar.",
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
