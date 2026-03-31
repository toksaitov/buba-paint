import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { OpenTrades } from "../open-trades";
import type { TradeRow } from "../../../lib/types";

function makeTrade(overrides: Partial<TradeRow> = {}): TradeRow {
  return {
    id: 1,
    strategy: "latency-arb",
    side: "UP",
    token_id: "token-1",
    size: 12.5,
    entry_price: 0.45,
    timestamp: 1_700_000_000_000,
    market_id: "market-1",
    status: "open",
    pnl: null,
    settlement_price: null,
    resolved_at: null,
    ...overrides,
  };
}

it("renders an empty state when no open trades exist", () => {
  render(<OpenTrades trades={[makeTrade({ status: "closed" })]} />);

  expect(screen.getByText("Open Trades")).toBeInTheDocument();
  expect(screen.getByText("No open trades")).toBeInTheDocument();
});

it("filters to open trades and renders their key fields", () => {
  render(
    <OpenTrades
      trades={[
        makeTrade(),
        makeTrade({
          id: 2,
          side: "DOWN",
          strategy: "spread-capture",
          status: "closed",
        }),
      ]}
    />,
  );

  expect(screen.getByText("latency-arb")).toBeInTheDocument();
  expect(screen.getByText("UP")).toBeInTheDocument();
  expect(screen.queryByText("spread-capture")).not.toBeInTheDocument();
});

