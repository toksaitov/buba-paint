import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint", bot: { id: "paint", name: "Paint" } }),
}));

vi.mock("../../hooks/use-bot-status", () => ({
  useBotStatus: vi.fn(),
}));

vi.mock("../../lib/api", () => ({
  getStats: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { StatsPage } from "../stats";
import { useBotStatus } from "../../hooks/use-bot-status";
import * as api from "../../lib/api";
const mockUseBotStatus = vi.mocked(useBotStatus);
const mockGetStats = vi.mocked(api.getStats);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function W({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockUseBotStatus.mockReturnValue({ data: undefined } as ReturnType<typeof useBotStatus>);
});

test("shows loading state", () => {
  mockGetStats.mockReturnValue(new Promise(() => {}));
  render(<StatsPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders data when loaded", async () => {
  mockGetStats.mockResolvedValue({
    by_strategy: {
      "latency-arb": { trades: 10, wins: 6, losses: 4, win_rate: 0.6, total_pnl: 300 },
    },
  });
  mockUseBotStatus.mockReturnValue({
    data: {
      balance: 500,
      execution_mode: "paper",
      live_session_status: null,
      uptime_hours: 48,
      open_trades: 1,
    },
  } as ReturnType<typeof useBotStatus>);

  render(<StatsPage />, { wrapper: createWrapper() });

  const heading = await screen.findByText("Bot Status");
  expect(heading).toBeDefined();
  expect(screen.getByText("60.0%")).toBeDefined();
});
