import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { RecentActivity } from "../recent-activity";
import type { TradeRow } from "../../../lib/types";

function makeTrade(overrides: Partial<TradeRow> = {}): TradeRow {
  return {
    id: 1,
    strategy: "latency-arb",
    side: "UP",
    token_id: "token-1",
    size: 10,
    entry_price: 0.44,
    timestamp: 1_700_000_000_000,
    market_id: "market-1",
    status: "closed",
    pnl: 7.5,
    settlement_price: 1,
    resolved_at: 1_700_000_100_000,
    ...overrides,
  };
}

it("renders an empty state when there are no settled trades", () => {
  render(<RecentActivity trades={[makeTrade({ resolved_at: null, pnl: null })]} />);

  expect(screen.getByText("No settled trades yet.")).toBeInTheDocument();
});

it("renders only settled trades and caps the list to eight items", () => {
  const trades = Array.from({ length: 10 }, (_, index) =>
    makeTrade({
      id: index + 1,
      strategy: `strategy-${index + 1}`,
      pnl: index - 4,
      resolved_at: 1_700_000_100_000 + index * 1_000,
    }),
  );

  render(<RecentActivity trades={trades} />);

  expect(screen.getByText("strategy-1")).toBeInTheDocument();
  expect(screen.getByText("strategy-8")).toBeInTheDocument();
  expect(screen.queryByText("strategy-9")).not.toBeInTheDocument();
  expect(screen.queryByText("strategy-10")).not.toBeInTheDocument();
});
