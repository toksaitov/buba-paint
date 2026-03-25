import { render, screen } from "@testing-library/react";
import { StatCard } from "../stat-card";

test("renders label and value", () => {
  render(<StatCard label="Balance" value="$500.00" />);
  expect(screen.getByText("Balance")).toBeDefined();
  expect(screen.getByText("$500.00")).toBeDefined();
});

test("renders sub when provided", () => {
  render(<StatCard label="PnL" value="+$100" sub="50% win rate" />);
  expect(screen.getByText("50% win rate")).toBeDefined();
});

test("applies color class", () => {
  render(<StatCard label="PnL" value="+$100" color="text-accent-green" />);
  const valueEl = screen.getByText("+$100");
  expect(valueEl.className).toContain("text-accent-green");
});
