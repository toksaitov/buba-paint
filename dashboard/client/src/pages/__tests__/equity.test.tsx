import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-equity-series", () => ({
  useEquitySeries: vi.fn(),
}));

vi.mock("../../hooks/use-trading-summary", () => ({
  useTradingSummary: vi.fn(),
}));

vi.mock("../../components/equity/equity-chart", () => ({
  EquityChart: () => <div data-testid="equity-chart">chart</div>,
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { EquityPage } from "../equity";
import { useEquitySeries } from "../../hooks/use-equity-series";
import { useTradingSummary } from "../../hooks/use-trading-summary";
const mockUseEquitySeries = vi.mocked(useEquitySeries);
const mockUseTradingSummary = vi.mocked(useTradingSummary);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function W({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockUseTradingSummary.mockReturnValue({
    data: {
      runtime_mode: "live_readonly",
      trading_state: "readonly",
      process_state: "running",
      venue_health: { state: "healthy", label: "Venue connected", detail: null },
      account_health: { state: "healthy", label: "Account tracked", detail: null },
      reconciliation_health: { state: "healthy", label: "Clean", detail: null },
      shadow_summary: {
        balance: 200,
        starting_balance: 150,
        total_pnl: 50,
        total_trades: 4,
        wins: 3,
        losses: 1,
        win_rate: 0.75,
        max_drawdown_pct: 0.12,
        high_water_mark: 210,
        uptime_hours: 3.5,
        open_trades: 1,
        current_window: null,
      },
      real_account_summary: {
        session_id: 1,
        session_status: "readonly_ready",
        session_started_at_ms: 1000,
        cash_cap_usd: 100,
        available_cash: 99,
        reserved_cash: 0,
        inventory_mark_value: 0,
        redeemable_value: 0,
        pending_redeem_value: 0,
        total_equity: 99,
        allowance_available: 99,
        latest_snapshot_at_ms: 1000,
        provider: "polymarket",
        user_stream_status: "ok",
        last_user_stream_connected_at_ms: 1000,
        last_user_stream_event_at_ms: null,
        last_account_refresh_at_ms: 1000,
        wallet_address: "0xabc",
        proxy_wallet: "0xdef",
        enabled_strategies: ["latency-arb"],
        config_fingerprint: "fingerprint",
      },
      capabilities: {
        preflight: { enabled: false, reason: "gated" },
        arm: { enabled: false, reason: "gated" },
        disarm: { enabled: false, reason: "gated" },
        cancel_all: { enabled: false, reason: "gated" },
        stop_after_flat: { enabled: false, reason: "gated" },
        redeem: { enabled: false, reason: "gated" },
        kill_switch: { enabled: false, reason: "gated" },
      },
      alerts: [],
    },
  } as ReturnType<typeof useTradingSummary>);
});

test("shows loading state", () => {
  mockUseEquitySeries.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useEquitySeries>);
  render(<EquityPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders data when loaded", () => {
  mockUseEquitySeries.mockReturnValue({
    isLoading: false,
    data: {
      baseline: { id: 1, timestamp: 0, event: "init", balance: 150 },
      points: [{ id: 2, timestamp: 1000, event: "settlement", balance: 200 }],
    },
  } as ReturnType<typeof useEquitySeries>);
  render(<EquityPage />, { wrapper: createWrapper() });
  expect(screen.getByText("Equity curve")).toBeDefined();
  expect(
    screen.getByText("Simulated balance over time. Starting balance is the baseline."),
  ).toBeInTheDocument();
  expect(screen.getByTestId("equity-chart")).toBeDefined();
});
