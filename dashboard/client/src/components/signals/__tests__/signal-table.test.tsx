import { render, screen } from "@testing-library/react";
import { SignalTable } from "../signal-table";
import type { SignalRow } from "../../../lib/types";

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

test("renders signal rows", () => {
  const signals = [makeSignal(), makeSignal({ id: 2, strategy: "spread-capture" })];
  render(<SignalTable signals={signals} />);
  expect(screen.getByText("latency-arb")).toBeDefined();
  expect(screen.getByText("spread-capture")).toBeDefined();
});

test("displays strategy and direction", () => {
  render(<SignalTable signals={[makeSignal({ direction: "DOWN" })]} />);
  expect(screen.getByText("DOWN")).toBeDefined();
  expect(screen.getByText("DOWN").className).toContain("text-accent-red");
});
