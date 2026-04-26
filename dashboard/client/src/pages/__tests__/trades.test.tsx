import { render, screen } from "@testing-library/react";
import { beforeEach, vi } from "vitest";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-trades", () => ({
  useTrades: vi.fn(),
}));

vi.mock("../../components/trades/trade-table", () => ({
  TradeTable: ({ trades }: { trades: unknown[] }) => (
    <div data-testid="trade-table">{trades.length} trades</div>
  ),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { useTrades } from "../../hooks/use-trades";
import { TradesPage } from "../trades";

const mockUseTrades = vi.mocked(useTrades);

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state", () => {
  mockUseTrades.mockReturnValue({
    isLoading: true,
    data: undefined,
  } as ReturnType<typeof useTrades>);
  render(<TradesPage />);
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders the shadow trade page", () => {
  mockUseTrades.mockReturnValue({
    isLoading: false,
    data: { trades: [{ id: 1 }], total: 1, page: 1, per_page: 50 },
  } as ReturnType<typeof useTrades>);
  render(<TradesPage />);
  expect(screen.getByTestId("trade-table")).toBeInTheDocument();
  expect(screen.getByText("Trade history")).toBeInTheDocument();
  expect(
    screen.getByText("Simulated trade history."),
  ).toBeInTheDocument();
});
