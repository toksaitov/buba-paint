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
  expect(screen.getByText("Live")).toBeDefined();
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

test("collapsed bot selector uses icon buttons with titles", async () => {
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

  const paperButton = screen.getByTitle("Paper");
  await user.click(paperButton);

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
      <Nav
        {...defaultProps}
        bots={[]}
        activeBotId=""
      />
    </MemoryRouter>,
  );

  expect(screen.queryByText("Bot")).toBeNull();
  expect(screen.getByText("Pages")).toBeInTheDocument();
});
