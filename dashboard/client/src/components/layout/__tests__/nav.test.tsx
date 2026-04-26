import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { vi } from "vitest";
import { Nav } from "../nav";

const defaultProps = {
  collapsed: false,
  bots: [{ id: "paint", name: "Paint" }],
  activeBotId: "paint",
  onSelectBot: vi.fn(),
};

test("renders grouped navigation with Execution near the top", () => {
  render(
    <MemoryRouter>
      <Nav {...defaultProps} />
    </MemoryRouter>,
  );

  expect(screen.getByText("Monitor")).toBeInTheDocument();
  expect(screen.getByText("Analysis")).toBeInTheDocument();
  expect(screen.getByText("Overview")).toBeInTheDocument();
  expect(screen.getByText("Execution")).toBeInTheDocument();
  expect(screen.getByText("Logs")).toBeInTheDocument();
  expect(screen.getByText("Equity")).toBeInTheDocument();
  expect(screen.getByText("Trades")).toBeInTheDocument();
  expect(screen.getByText("Signals")).toBeInTheDocument();
  expect(screen.getByText("Strategies")).toBeInTheDocument();
});

test("highlights the active execution route", () => {
  render(
    <MemoryRouter initialEntries={["/execution"]}>
      <Nav {...defaultProps} />
    </MemoryRouter>,
  );

  const execution = screen.getByText("Execution").closest("a");
  expect(execution?.className).toContain("bg-text");
});

test("collapsed mode hides labels", () => {
  render(
    <MemoryRouter>
      <Nav {...defaultProps} collapsed={true} />
    </MemoryRouter>,
  );

  expect(screen.queryByText("Overview")).toBeNull();
  expect(screen.queryByText("Execution")).toBeNull();
});

test("collapsed bot selector uses titled icon buttons", async () => {
  const onSelectBot = vi.fn();
  const user = userEvent.setup();

  render(
    <MemoryRouter>
      <Nav
        {...defaultProps}
        collapsed={true}
        bots={[
          { id: "paint", name: "Paint" },
          { id: "paper", name: "Paper" },
        ]}
        onSelectBot={onSelectBot}
      />
    </MemoryRouter>,
  );

  await user.click(screen.getByTitle("Paper"));
  expect(onSelectBot).toHaveBeenCalledWith("paper");
});

test("page navigation calls onNavigate when provided", async () => {
  const onNavigate = vi.fn();
  const user = userEvent.setup();

  render(
    <MemoryRouter>
      <Nav {...defaultProps} onNavigate={onNavigate} />
    </MemoryRouter>,
  );

  await user.click(screen.getByText("Trades"));
  expect(onNavigate).toHaveBeenCalledTimes(1);
});

test("omits the bot section when no bots are available", () => {
  render(
    <MemoryRouter>
      <Nav {...defaultProps} bots={[]} activeBotId="" />
    </MemoryRouter>,
  );

  expect(screen.queryByText("Bot")).toBeNull();
  expect(screen.getByText("Monitor")).toBeInTheDocument();
});
