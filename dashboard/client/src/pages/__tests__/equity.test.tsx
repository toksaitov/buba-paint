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

vi.mock("../../components/equity/equity-chart", () => ({
  EquityChart: () => <div data-testid="equity-chart">chart</div>,
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { EquityPage } from "../equity";
import { useEquitySeries } from "../../hooks/use-equity-series";
const mockUseEquitySeries = vi.mocked(useEquitySeries);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function W({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state", () => {
  mockUseEquitySeries.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useEquitySeries>);
  render(<EquityPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders the chart when data loaded", () => {
  mockUseEquitySeries.mockReturnValue({
    isLoading: false,
    data: {
      baseline: { id: 1, timestamp: 0, event: "init", balance: 150 },
      points: [{ id: 2, timestamp: 1000, event: "settlement", balance: 200 }],
    },
  } as ReturnType<typeof useEquitySeries>);
  render(<EquityPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("equity-chart")).toBeDefined();
});
