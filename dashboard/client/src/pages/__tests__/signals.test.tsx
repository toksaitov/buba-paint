import { render, screen } from "@testing-library/react";
import { vi, beforeEach } from "vitest";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-signals", () => ({
  useSignals: vi.fn(),
}));

vi.mock("../../hooks/use-bot-status", () => ({
  useBotStatus: vi.fn(),
}));

vi.mock("../../components/signals/signal-table", () => ({
  SignalTable: ({ signals }: { signals: unknown[] }) => (
    <div data-testid="signal-table">{signals.length} signals</div>
  ),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { SignalsPage } from "../signals";
import { useBotStatus } from "../../hooks/use-bot-status";
import { useSignals } from "../../hooks/use-signals";
const mockUseBotStatus = vi.mocked(useBotStatus);
const mockUseSignals = vi.mocked(useSignals);

beforeEach(() => {
  vi.clearAllMocks();
  mockUseBotStatus.mockReturnValue({ data: { execution_mode: "paper" } } as ReturnType<typeof useBotStatus>);
});

test("shows loading state", () => {
  mockUseSignals.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useSignals>);
  render(<SignalsPage />);
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders data when loaded", () => {
  mockUseSignals.mockReturnValue({
    isLoading: false,
    data: { signals: [{ id: 1 }] },
  } as ReturnType<typeof useSignals>);
  render(<SignalsPage />);
  expect(screen.getByText("Signal Log")).toBeDefined();
  expect(screen.getByTestId("signal-table")).toBeDefined();
});
