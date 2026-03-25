import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-balance", () => ({
  useBalance: vi.fn(),
}));

vi.mock("../../hooks/use-bot-status", () => ({
  useBotStatus: vi.fn(),
}));

vi.mock("../../components/equity/equity-chart", () => ({
  EquityChart: () => <div data-testid="equity-chart">chart</div>,
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { EquityPage } from "../equity";
import { useBalance } from "../../hooks/use-balance";
import { useBotStatus } from "../../hooks/use-bot-status";
const mockUseBalance = vi.mocked(useBalance);
const mockUseBotStatus = vi.mocked(useBotStatus);

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
  mockUseBalance.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useBalance>);
  render(<EquityPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders data when loaded", () => {
  mockUseBalance.mockReturnValue({
    isLoading: false,
    data: { entries: [{ id: 1, timestamp: 1000, event: "init", balance: 200 }] },
  } as ReturnType<typeof useBalance>);
  render(<EquityPage />, { wrapper: createWrapper() });
  expect(screen.getByText("Equity Curve")).toBeDefined();
  expect(screen.getByTestId("equity-chart")).toBeDefined();
});
