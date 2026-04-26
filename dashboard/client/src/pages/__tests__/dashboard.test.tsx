import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
  Link: ({ children, to }: { children: ReactNode; to: string }) => (
    <a href={to}>{children}</a>
  ),
}));

vi.mock("../../hooks/use-trading-summary", () => ({
  useTradingSummary: vi.fn(),
}));

vi.mock("../../hooks/use-trades", () => ({
  useTrades: vi.fn(),
}));

vi.mock("../../components/dashboard/open-trades", () => ({
  OpenTrades: () => <div data-testid="open-trades">open-trades</div>,
}));

vi.mock("../../components/dashboard/recent-activity", () => ({
  RecentActivity: () => <div data-testid="recent-activity">recent-activity</div>,
}));

vi.mock("../../components/dashboard/mini-chart", () => ({
  MiniChart: () => <div data-testid="mini-chart">mini-chart</div>,
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { DashboardPage } from "../dashboard";
import { useTrades } from "../../hooks/use-trades";
import { useTradingSummary } from "../../hooks/use-trading-summary";

const mockUseTradingSummary = vi.mocked(useTradingSummary);
const mockUseTrades = vi.mocked(useTrades);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockUseTrades.mockReturnValue({
    data: { trades: [], total: 0, page: 1, per_page: 20 },
  } as ReturnType<typeof useTrades>);
});

test("shows loading while fetching", () => {
  mockUseTradingSummary.mockReturnValue({
    isLoading: true,
    data: undefined,
  } as ReturnType<typeof useTradingSummary>);

  render(<DashboardPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders overview as the primary shadow summary surface", () => {
  mockUseTradingSummary.mockReturnValue({
    isLoading: false,
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
        balance: 134.07,
        starting_balance: 100,
        total_pnl: 34.9,
        total_trades: 10,
        wins: 6,
        losses: 4,
        win_rate: 0.6,
        open_trades: 1,
        uptime_hours: 48.5,
        high_water_mark: 150,
        max_drawdown_pct: 0.15,
        live_session_status: "readonly_ready",
        last_tick_at: null,
        current_window: {
          market_id: "mkt-1",
          question: "Will BTC go up?",
          end_time: 1_716_000_300_000,
        },
      },
      real_account_summary: {
        available_cash: 99.17,
        reserved_cash: 0,
        inventory_mark_value: 0,
        redeemable_value: 0,
        pending_redeem_value: 0,
        total_equity: 99.17,
        allowance_available: 99.17,
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

  render(<DashboardPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Balance")).toBeInTheDocument();
  expect(screen.getByText("Current market")).toBeInTheDocument();
  expect(screen.getByText("Open shadow trades")).toBeInTheDocument();
  expect(screen.getByText("Polymarket account")).toBeInTheDocument();
  expect(screen.getByText("Recent settled trades")).toBeInTheDocument();
  expect(screen.getByTestId("mini-chart")).toBeInTheDocument();
  expect(screen.getByTestId("open-trades")).toBeInTheDocument();
  expect(screen.getByTestId("recent-activity")).toBeInTheDocument();
  expect(screen.queryByText("Trading state")).not.toBeInTheDocument();
  expect(screen.queryByText("Real account")).not.toBeInTheDocument();
  expect(screen.queryByText("Open positions")).not.toBeInTheDocument();
  expect(screen.queryByText("Provider")).not.toBeInTheDocument();
  expect(screen.queryByText("Strategies")).not.toBeInTheDocument();

  const headings = screen.getAllByRole("heading", { level: 2 }).map((node) => node.textContent);
  expect(headings.indexOf("Current market")).toBeLessThan(headings.indexOf("Open shadow trades"));
  expect(headings.indexOf("Open shadow trades")).toBeLessThan(headings.indexOf("Polymarket account"));
});

test("does not render alerts on overview when none exist", () => {
  mockUseTradingSummary.mockReturnValue({
    isLoading: false,
    data: {
      runtime_mode: "paper",
      trading_state: "paper",
      process_state: "running",
      venue_health: { state: "unavailable", label: "Unavailable", detail: null },
      account_health: { state: "unavailable", label: "Unavailable", detail: null },
      reconciliation_health: { state: "unavailable", label: "Unavailable", detail: null },
      shadow_summary: {
        balance: 200,
        starting_balance: 200,
        total_pnl: 0,
        total_trades: 0,
        wins: 0,
        losses: 0,
        win_rate: 0,
        open_trades: 0,
        uptime_hours: 1,
        high_water_mark: 200,
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
    },
  } as ReturnType<typeof useTradingSummary>);

  render(<DashboardPage />, { wrapper: createWrapper() });

  expect(screen.queryByText("Alerts")).not.toBeInTheDocument();
});
