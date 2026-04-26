import { render, screen } from "@testing-library/react";
import { beforeEach, vi } from "vitest";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint" }),
}));

vi.mock("../../hooks/use-signals", () => ({
  useSignals: vi.fn(),
}));

vi.mock("../../hooks/use-signal-groups", () => ({
  useSignalGroups: vi.fn(),
}));

vi.mock("../../components/signals/signal-table", () => ({
  SignalTable: ({ signals }: { signals: unknown[] }) => (
    <div data-testid="signal-table">{signals.length} signals</div>
  ),
  SignalGroupTable: ({ groups }: { groups: unknown[] }) => (
    <div data-testid="signal-group-table">{groups.length} groups</div>
  ),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { useSignals } from "../../hooks/use-signals";
import { useSignalGroups } from "../../hooks/use-signal-groups";
import { SignalsPage } from "../signals";

const mockUseSignals = vi.mocked(useSignals);
const mockUseSignalGroups = vi.mocked(useSignalGroups);

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state", () => {
  mockUseSignalGroups.mockReturnValue({
    isLoading: true,
    data: undefined,
  } as ReturnType<typeof useSignalGroups>);
  mockUseSignals.mockReturnValue({
    isLoading: false,
    data: undefined,
  } as ReturnType<typeof useSignals>);
  render(<SignalsPage />);
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders the shadow signal page", () => {
  mockUseSignalGroups.mockReturnValue({
    isLoading: false,
    data: { groups: [{ id: "g1" }], raw_rows_scanned: 3, quiet_gap_ms: 5000 },
  } as ReturnType<typeof useSignalGroups>);
  mockUseSignals.mockReturnValue({
    isLoading: false,
    data: { signals: [{ id: 1 }] },
  } as ReturnType<typeof useSignals>);
  render(<SignalsPage />);
  expect(screen.getByText("Signals")).toBeInTheDocument();
  expect(
    screen.getByText(/Signals grouped into bursts.*quiet gap 5000ms.*scanned 3 rows/),
  ).toBeInTheDocument();
  expect(screen.getByTestId("signal-group-table")).toBeInTheDocument();
});
