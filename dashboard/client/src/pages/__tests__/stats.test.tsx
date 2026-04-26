import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-trading-summary", () => ({
  useTradingSummary: vi.fn(),
}));

vi.mock("../../lib/api", () => ({
  getStats: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { getStats } from "../../lib/api";
import { useTradingSummary } from "../../hooks/use-trading-summary";
import { StatsPage } from "../stats";

const mockUseTradingSummary = vi.mocked(useTradingSummary);
const mockGetStats = vi.mocked(getStats);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state", () => {
  mockUseTradingSummary.mockReturnValue({
    data: undefined,
  } as ReturnType<typeof useTradingSummary>);
  mockGetStats.mockReturnValue(new Promise(() => {}));
  render(<StatsPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders stats page without a duplicated overview summary deck", async () => {
  mockUseTradingSummary.mockReturnValue({
    data: {
      runtime_mode: "live_readonly",
      trading_state: "readonly",
      process_state: "running",
      venue_health: { state: "healthy", label: "Venue connected", detail: null },
      account_health: { state: "healthy", label: "Account tracked", detail: null },
      reconciliation_health: {
        state: "healthy",
        label: "Reconciliation clean",
        detail: null,
      },
      shadow_summary: {
        balance: 500,
        starting_balance: 200,
        total_pnl: 300,
        total_trades: 10,
        wins: 6,
        losses: 4,
        win_rate: 0.6,
        open_trades: 1,
        uptime_hours: 48,
        high_water_mark: 550,
        max_drawdown_pct: 0.15,
        live_session_status: "readonly_ready",
        last_tick_at: null,
        current_window: null,
      },
      real_account_summary: {
        available_cash: 99,
        reserved_cash: 0,
        inventory_mark_value: 0,
        redeemable_value: 0,
        pending_redeem_value: 0,
        total_equity: 99,
        allowance_available: 99,
        latest_snapshot_at_ms: null,
        session_id: 1,
        session_status: "readonly_ready",
        session_started_at_ms: null,
        wallet_address: null,
        proxy_wallet: null,
        cash_cap_usd: 100,
        enabled_strategies: ["latency-arb"],
        provider: "polymarket",
        user_stream_status: "ok",
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
    },
  } as ReturnType<typeof useTradingSummary>);
  mockGetStats.mockResolvedValue({
    by_strategy: {
      "latency-arb": {
        trades: 10,
        wins: 6,
        losses: 4,
        win_rate: 0.6,
        total_pnl: 300,
      },
    },
  });

  render(<StatsPage />, { wrapper: createWrapper() });

  expect(await screen.findByText("Strategies")).toBeInTheDocument();
  expect(screen.getByText("Strategy contribution")).toBeInTheDocument();
  expect(screen.queryByText("Risk and drawdown")).not.toBeInTheDocument();
  expect(screen.getAllByText("latency-arb").length).toBeGreaterThan(0);
  expect(screen.queryByText("Runtime summary")).not.toBeInTheDocument();
});
