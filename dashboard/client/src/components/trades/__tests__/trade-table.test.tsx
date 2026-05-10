import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import { TradeTable } from "../trade-table";
import type { TradeRow } from "../../../lib/types";

function makeTrade(overrides: Partial<TradeRow> = {}): TradeRow {
  return {
    id: 1,
    strategy: "latency-arb",
    side: "UP",
    token_id: "tok-up",
    size: 50,
    entry_price: 0.45,
    timestamp: 1767267045000,
    market_id: "mkt-1",
    status: "closed",
    pnl: 55.0,
    settlement_price: 1.0,
    resolved_at: 1767267345000,
    ...overrides,
  };
}

beforeEach(() => {
  localStorage.clear();
});

test("renders trade rows", () => {
  const trades = [makeTrade(), makeTrade({ id: 2, strategy: "spread-capture" })];
  render(
    <TradeTable trades={trades} page={1} total={2} perPage={50} onPageChange={vi.fn()} />,
  );
  expect(screen.getAllByText("latency-arb").length).toBeGreaterThanOrEqual(1);
  expect(screen.getAllByText("spread-capture").length).toBeGreaterThanOrEqual(1);
});

test("shows dash for unsettled trades", () => {
  const trades = [makeTrade({ pnl: null, settlement_price: null })];
  render(
    <TradeTable trades={trades} page={1} total={1} perPage={50} onPageChange={vi.fn()} />,
  );
  const dashes = screen.getAllByText("--");
  expect(dashes.length).toBeGreaterThanOrEqual(2);
});

test("formats pnl with color", () => {
  const trades = [makeTrade({ pnl: 55 }), makeTrade({ id: 2, pnl: -30 })];
  render(
    <TradeTable trades={trades} page={1} total={2} perPage={50} onPageChange={vi.fn()} />,
  );
  const cells = screen.getAllByText(/\$/);

  const pnlPositive = cells.find((el) => el.textContent?.includes("+"));
  const pnlNegative = cells.find((el) => el.textContent?.includes("-"));
  expect(pnlPositive?.className).toContain("text-accent-green");
  expect(pnlNegative?.className).toContain("text-accent-red");
});

test("pagination visible when multi-page", () => {
  const trades = [makeTrade()];
  render(
    <TradeTable trades={trades} page={1} total={100} perPage={50} onPageChange={vi.fn()} />,
  );
  expect(screen.getByText("Prev")).toBeDefined();
  expect(screen.getByText("Next")).toBeDefined();
});

test("prev disabled on first page", () => {
  render(
    <TradeTable trades={[makeTrade()]} page={1} total={100} perPage={50} onPageChange={vi.fn()} />,
  );
  const prev = screen.getByText("Prev");
  expect((prev as HTMLButtonElement).disabled).toBe(true);
});

test("next disabled on last page", () => {
  render(
    <TradeTable trades={[makeTrade()]} page={2} total={100} perPage={50} onPageChange={vi.fn()} />,
  );
  const next = screen.getByText("Next");
  expect((next as HTMLButtonElement).disabled).toBe(true);
});

test("onPageChange called correctly", () => {
  const onPageChange = vi.fn();
  render(
    <TradeTable trades={[makeTrade()]} page={1} total={100} perPage={50} onPageChange={onPageChange} />,
  );
  fireEvent.click(screen.getByText("Next"));
  expect(onPageChange).toHaveBeenCalledWith(2);
});

test("remembers trade filters after remount", async () => {
  const user = userEvent.setup();
  const trades = [
    makeTrade(),
    makeTrade({
      id: 2,
      strategy: "spread-capture",
      side: "DOWN",
      market_id: "mkt-2",
      status: "open",
    }),
  ];
  const firstRender = render(
    <TradeTable trades={trades} page={1} total={2} perPage={50} onPageChange={vi.fn()} />,
  );

  await user.selectOptions(screen.getByLabelText("Strategy"), "spread-capture");
  await user.selectOptions(screen.getByLabelText("Side"), "DOWN");
  await user.selectOptions(screen.getByLabelText("Status"), "open");
  await user.type(screen.getByPlaceholderText("Market"), "mkt-2");
  firstRender.unmount();

  render(
    <TradeTable trades={trades} page={1} total={2} perPage={50} onPageChange={vi.fn()} />,
  );

  expect(screen.getByLabelText("Strategy")).toHaveValue("spread-capture");
  expect(screen.getByLabelText("Side")).toHaveValue("DOWN");
  expect(screen.getByLabelText("Status")).toHaveValue("open");
  expect(screen.getByPlaceholderText("Market")).toHaveValue("mkt-2");
  expect(screen.getByText("1 of 2 trades")).toBeInTheDocument();
  expect(screen.getAllByText("spread-capture").length).toBeGreaterThanOrEqual(1);
});
