import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import { useTradingSummary } from "../use-trading-summary";

vi.mock("../../lib/api", () => ({
  getTradingSummary: vi.fn(),
}));

import { getTradingSummary } from "../../lib/api";

const mockGetTradingSummary = vi.mocked(getTradingSummary);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

afterEach(() => {
  vi.clearAllMocks();
});

test("returns trading summary data on success", async () => {
  mockGetTradingSummary.mockResolvedValue({
    runtime_mode: "paper",
    trading_state: "paper",
    process_state: "running",
    venue_health: { state: "idle", label: "Paper only", detail: null },
    account_health: { state: "idle", label: "Shadow only", detail: null },
    reconciliation_health: { state: "idle", label: "Paper only", detail: null },
    shadow_summary: {
      balance: 100,
      starting_balance: 100,
      total_pnl: 0,
      total_trades: 0,
      wins: 0,
      losses: 0,
      win_rate: 0,
      open_trades: 0,
      uptime_hours: 0,
      high_water_mark: 100,
      max_drawdown_pct: 0,
      live_session_status: null,
      last_tick_at: null,
      current_window: null,
    },
    real_account_summary: {
      available_cash: null,
      reserved_cash: null,
      inventory_mark_value: null,
      redeemable_value: null,
      pending_redeem_value: null,
      total_equity: null,
      allowance_available: null,
      latest_snapshot_at_ms: null,
      session_id: null,
      session_status: null,
      session_started_at_ms: null,
      wallet_address: null,
      proxy_wallet: null,
      cash_cap_usd: null,
      enabled_strategies: [],
      provider: null,
      user_stream_status: null,
      last_user_stream_connected_at_ms: null,
      last_user_stream_event_at_ms: null,
      last_account_refresh_at_ms: null,
      open_orders: 0,
      pending_redemptions: 0,
      critical_reconciliation_events: 0,
    },
    capabilities: {
      preflight: { enabled: false, reason: "disabled" },
      arm: { enabled: false, reason: "disabled" },
      disarm: { enabled: false, reason: "disabled" },
      cancel_all: { enabled: false, reason: "disabled" },
      stop_after_flat: { enabled: false, reason: "disabled" },
      redeem: { enabled: false, reason: "disabled" },
      kill_switch: { enabled: false, reason: "disabled" },
    },
    alerts: [],
  });

  const { result } = renderHook(() => useTradingSummary("bot-1"), {
    wrapper: createWrapper(),
  });

  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.runtime_mode).toBe("paper");
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useTradingSummary(""), {
    wrapper: createWrapper(),
  });

  expect(result.current.isFetching).toBe(false);
  expect(mockGetTradingSummary).not.toHaveBeenCalled();
});
