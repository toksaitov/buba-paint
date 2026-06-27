import { EventEmitter } from "node:events";
import { afterEach, describe, expect, it, vi } from "vitest";
import { startSidecar } from "../index.js";
import { loadConfig } from "../config.js";
import type {
  LiveAccountState,
  LiveActivityResponse,
  LiveCancellationResponse,
  LiveOrderIntentRequest,
  LiveOrderIntentResponse,
  LivePreflightRequest,
  LivePreflightResponse,
  LiveRedemptionResponse,
} from "../types.js";

class FakeProcess extends EventEmitter {
  exit = vi.fn((_: number | undefined) => undefined as never);
}

function createNoopProvider(close = vi.fn(async () => undefined)) {
  return {
    health: async () => ({
      ok: true,
      ready: false,
      readiness_status: "failed" as const,
      mode: "local-sidecar",
      provider: "test",
      signature_type: 1,
      wallet_address: null,
      proxy_wallet: null,
      auth_configured: true,
      relayer_api_key_present: false,
      user_stream_status: "failed" as const,
      last_user_stream_connected_at_ms: null,
      last_user_stream_event_at_ms: null,
      last_user_stream_error: null,
      last_successful_account_refresh_at_ms: null,
      last_account_refresh_error: null,
      last_user_stream_disconnect_at_ms: null,
      last_user_stream_disconnect_reason: null,
      consecutive_user_stream_failures: 0,
    }),
    accountState: async (): Promise<LiveAccountState> => {
      throw new Error("unexpected");
    },
    preflight: async (_request: LivePreflightRequest): Promise<LivePreflightResponse> => {
      throw new Error("unexpected");
    },
    submitOrderIntent: async (
      _request: LiveOrderIntentRequest,
    ): Promise<LiveOrderIntentResponse> => {
      throw new Error("unexpected");
    },
    cancelOrder: async (_orderId: string): Promise<LiveCancellationResponse> => {
      throw new Error("unexpected");
    },
    cancelAll: async (): Promise<LiveCancellationResponse> => {
      throw new Error("unexpected");
    },
    redeemAll: async (): Promise<LiveRedemptionResponse> => {
      throw new Error("unexpected");
    },
    onchainPosition: async () => {
      throw new Error("unexpected");
    },
    activity: async (): Promise<LiveActivityResponse> => ({
      timestamp_ms: Date.now(),
      user_stream_status: "failed",
      user_stream_events: [],
      clob_trades: [],
      details_json: JSON.stringify({ source: "test" }),
    }),
    close,
  };
}

describe("startSidecar", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("handles SIGTERM without hanging and closes the provider", async () => {
    const proc = new FakeProcess();
    const providerClose = vi.fn(async () => undefined);
    const runtime = startSidecar(
      loadConfig({ SIDECAR_PORT: "0" }),
      createNoopProvider(providerClose),
      proc,
    );

    proc.emit("SIGTERM");
    await vi.waitFor(() => {
      expect(providerClose).toHaveBeenCalledTimes(1);
      expect(proc.exit).toHaveBeenCalledWith(0);
    });
    await runtime.close().catch(() => undefined);
  });

  it("exits nonzero on an uncaught exception", async () => {
    const proc = new FakeProcess();
    const providerClose = vi.fn(async () => undefined);
    const runtime = startSidecar(
      loadConfig({ SIDECAR_PORT: "0" }),
      createNoopProvider(providerClose),
      proc,
    );

    proc.emit("uncaughtException", new Error("boom"));
    await vi.waitFor(() => {
      expect(providerClose).toHaveBeenCalledTimes(1);
      expect(proc.exit).toHaveBeenCalledWith(1);
    });
    await runtime.close().catch(() => undefined);
  });
});
