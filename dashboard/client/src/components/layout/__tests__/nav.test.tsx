import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { vi } from "vitest";
import { Nav } from "../nav";

const defaultProps = {
  collapsed: false,
  bots: [{ id: "paint", name: "Paint" }],
  activeBotId: "paint",
  onSelectBot: vi.fn(),
};

test("renders all nav links", () => {
  render(
    <MemoryRouter>
      <Nav {...defaultProps} />
    </MemoryRouter>,
  );
  expect(screen.getByText("Overview")).toBeDefined();
  expect(screen.getByText("Equity")).toBeDefined();
  expect(screen.getByText("Trades")).toBeDefined();
  expect(screen.getByText("Signals")).toBeDefined();
  expect(screen.getByText("Logs")).toBeDefined();
  expect(screen.getByText("Stats")).toBeDefined();
});

test("highlights active route", () => {
  render(
    <MemoryRouter initialEntries={["/"]}>
      <Nav {...defaultProps} />
    </MemoryRouter>,
  );
  const overview = screen.getByText("Overview").closest("a");
  expect(overview?.className).toContain("bg-text");
});

test("collapsed hides labels", () => {
  render(
    <MemoryRouter>
      <Nav {...defaultProps} collapsed={true} />
    </MemoryRouter>,
  );
  expect(screen.queryByText("Overview")).toBeNull();
  expect(screen.queryByText("Trades")).toBeNull();
});
