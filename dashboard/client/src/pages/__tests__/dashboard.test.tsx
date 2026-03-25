import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-bot-status", () => ({
  useBotStatus: vi.fn(),
}));

vi.mock("../../hooks/use-trades", () => ({
  useTrades: vi.fn(),
}));

vi.mock("../../hooks/use-balance", () => ({
  useBalance: vi.fn(),
}));

vi.mock("../../components/dashboard/stat-card", () => ({
  StatCard: ({ label, value }: { label: string; value: string }) => (
    <div data-testid="stat-card">{label}: {value}</div>
  ),
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
import { useBotStatus } from "../../hooks/use-bot-status";
import { useTrades } from "../../hooks/use-trades";
import { useBalance } from "../../hooks/use-balance";

const mockUseBotStatus = vi.mocked(useBotStatus);
const mockUseTrades = vi.mocked(useTrades);
const mockUseBalance = vi.mocked(useBalance);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function W({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

const mockStatus = {
  balance: 500,
  starting_balance: 200,
  total_trades: 10,
  wins: 6,
  losses: 4,
  win_rate: 60,
  total_pnl: 300,
  max_drawdown_pct: 15,
  high_water_mark: 550,
  uptime_hours: 48.5,
  current_window: null,
  open_trades: 1,
  last_tick_at: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  mockUseTrades.mockReturnValue({ data: { trades: [], total: 0, page: 1, per_page: 20 } } as ReturnType<typeof useTrades>);
  mockUseBalance.mockReturnValue({ data: { entries: [] } } as ReturnType<typeof useBalance>);
});

test("shows loading while fetching", () => {
  mockUseBotStatus.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useBotStatus>);
  render(<DashboardPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders stat cards", () => {
  mockUseBotStatus.mockReturnValue({ isLoading: false, data: mockStatus } as ReturnType<typeof useBotStatus>);
  render(<DashboardPage />, { wrapper: createWrapper() });
  const cards = screen.getAllByTestId("stat-card");
  expect(cards.length).toBe(4);
});

test("renders open trades component", () => {
  mockUseBotStatus.mockReturnValue({ isLoading: false, data: mockStatus } as ReturnType<typeof useBotStatus>);
  render(<DashboardPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("open-trades")).toBeDefined();
});

test("renders recent activity", () => {
  mockUseBotStatus.mockReturnValue({ isLoading: false, data: mockStatus } as ReturnType<typeof useBotStatus>);
  render(<DashboardPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("recent-activity")).toBeDefined();
});

test("renders mini chart", () => {
  mockUseBotStatus.mockReturnValue({ isLoading: false, data: mockStatus } as ReturnType<typeof useBotStatus>);
  render(<DashboardPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("mini-chart")).toBeDefined();
});
