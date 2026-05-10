import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach } from "vitest";
import { SignalGroupTable, SignalTable } from "../signal-table";
import type { SignalGroupRow, SignalRow } from "../../../lib/types";

function makeSignal(overrides: Partial<SignalRow> = {}): SignalRow {
  return {
    id: 1,
    timestamp: 1767267045000,
    strategy: "latency-arb",
    direction: "UP",
    binance_price: 42000,
    chainlink_price: 42001,
    up_ask: 0.45,
    down_ask: 0.55,
    metadata: '{"momentum": 0.001234}',
    ...overrides,
  };
}

function makeSignalGroup(overrides: Partial<SignalGroupRow> = {}): SignalGroupRow {
  return {
    id: "group-1",
    strategy: "latency-arb",
    direction: "UP",
    market_id: "mkt-1",
    start_timestamp: 1767267045000,
    end_timestamp: 1767267047000,
    count: 3,
    first_signal_id: 1,
    last_signal_id: 3,
    binance_price: 42000,
    chainlink_price: 42001,
    up_ask: 0.45,
    down_ask: 0.55,
    execution_fidelity: null,
    ...overrides,
  };
}

beforeEach(() => {
  localStorage.clear();
});

test("renders signal rows", () => {
  const signals = [makeSignal(), makeSignal({ id: 2, strategy: "spread-capture" })];
  render(<SignalTable signals={signals} />);
  expect(screen.getAllByText("latency-arb").length).toBeGreaterThanOrEqual(1);
  expect(screen.getAllByText("spread-capture").length).toBeGreaterThanOrEqual(1);
});

test("displays strategy and direction", () => {
  render(<SignalTable signals={[makeSignal({ direction: "DOWN" })]} />);
  const downs = screen.getAllByText("DOWN");
  expect(downs.length).toBeGreaterThanOrEqual(1);
  expect(downs[0].className).toContain("text-accent-red");
});

test("remembers grouped signal filters after remount", async () => {
  const user = userEvent.setup();
  const groups = [
    makeSignalGroup(),
    makeSignalGroup({
      id: "group-2",
      strategy: "spread-capture",
      direction: "DOWN",
      market_id: "mkt-2",
    }),
  ];
  const firstRender = render(<SignalGroupTable groups={groups} />);

  await user.selectOptions(screen.getByLabelText("Strategy"), "spread-capture");
  await user.selectOptions(screen.getByLabelText("Direction"), "DOWN");
  firstRender.unmount();

  render(<SignalGroupTable groups={groups} />);

  expect(screen.getByLabelText("Strategy")).toHaveValue("spread-capture");
  expect(screen.getByLabelText("Direction")).toHaveValue("DOWN");
  expect(screen.getByText("1 of 2 bursts")).toBeInTheDocument();
  expect(screen.getAllByText("spread-capture").length).toBeGreaterThanOrEqual(1);
});
