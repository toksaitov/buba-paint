import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Bot } from "../../../lib/types";
import { useMobileNavStore } from "../../../stores/mobile-nav-store";

const getBotsMock = vi.fn<() => Promise<{ bots: Bot[] }>>();
const liveUpdatesMock = vi.fn<(botId: string) => void>();
const useMediaQueryMock = vi.fn<(query: string) => boolean>();
const useTradingSummaryMock = vi.fn<(botId: string) => unknown>();

vi.mock("../nav", () => ({
  Nav: ({
    bots,
    activeBotId,
    onSelectBot,
    collapsed,
    onNavigate,
  }: {
    bots: Bot[];
    activeBotId: string;
    onSelectBot: (id: string) => void;
    collapsed: boolean;
    onNavigate?: () => void;
  }) => (
    <div data-testid="nav">
      <div data-testid="active-bot">{activeBotId}</div>
      <div data-testid="nav-collapsed">{String(collapsed)}</div>
      {bots.map((bot) => (
        <button key={bot.id} onClick={() => onSelectBot(bot.id)}>
          {bot.name}
        </button>
      ))}
      {onNavigate && <button onClick={onNavigate}>navigate</button>}
    </div>
  ),
}));

vi.mock("../header", () => ({
  Header: ({
    bot,
    botId,
    collapsed,
    onToggle,
    isDesktop,
  }: {
    bot: Bot | null;
    botId: string;
    collapsed: boolean;
    onToggle: () => void;
    isDesktop: boolean;
  }) => (
    <div data-testid="header">
      <span data-testid="header-bot-id">{botId}</span>
      <span data-testid="header-bot-name">{bot?.name ?? "none"}</span>
      <span data-testid="header-collapsed">{String(collapsed)}</span>
      <span data-testid="header-desktop">{String(isDesktop)}</span>
      <button onClick={onToggle}>toggle</button>
    </div>
  ),
}));

vi.mock("../logo", () => ({
  Logo: () => <div data-testid="logo">logo</div>,
}));

vi.mock("../../../lib/api", () => ({
  getBots: () => getBotsMock(),
}));

vi.mock("../../../hooks/use-live-updates", () => ({
  useLiveUpdates: (botId: string) => liveUpdatesMock(botId),
}));

vi.mock("../../../hooks/use-media-query", () => ({
  useMediaQuery: (query: string) => useMediaQueryMock(query),
}));

vi.mock("../../../hooks/use-trading-summary", () => ({
  useTradingSummary: (botId: string) => useTradingSummaryMock(botId),
}));

import { AppShell } from "../app-shell";

function renderShell(initialEntries: string[] = ["/"]) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });

  return {
    queryClient,
    ...render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={initialEntries}>
        <Routes>
          <Route element={<AppShell />}>
            <Route path="/" element={<div data-testid="outlet">overview</div>} />
            <Route path="/trades" element={<div data-testid="outlet">trades</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
    ),
  };
}

const bots: Bot[] = [
  { id: "bot-1", name: "Paint One" },
  { id: "bot-2", name: "Paint Two" },
];

beforeEach(() => {
  sessionStorage.clear();
  useMobileNavStore.setState({ isOpen: false });
  getBotsMock.mockReset();
  liveUpdatesMock.mockReset();
  useMediaQueryMock.mockReset();
  useTradingSummaryMock.mockReset();
  getBotsMock.mockResolvedValue({ bots });
  useMediaQueryMock.mockReturnValue(true);
  useTradingSummaryMock.mockReturnValue({
    data: {
      runtime_mode: "live_readonly",
      trading_state: "readonly",
    },
  });
});

it("auto-selects the first bot and persists it", async () => {
  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-1");
  });

  expect(screen.getByTestId("header-bot-name")).toHaveTextContent("Paint One");
  await waitFor(() => {
    expect(sessionStorage.getItem("activeBotId")).toBe("bot-1");
  });
  expect(liveUpdatesMock).toHaveBeenLastCalledWith("bot-1");
  expect(screen.getByText("Overview")).toBeInTheDocument();
  expect(
    screen.getByText(
      "Simulated performance and a quick look at the Polymarket account. Open Execution for venue detail.",
    ),
  ).toBeInTheDocument();
});

it("keeps a valid persisted bot selection", async () => {
  sessionStorage.setItem("activeBotId", "bot-2");

  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-2");
  });

  expect(screen.getByTestId("header-bot-name")).toHaveTextContent("Paint Two");
  expect(liveUpdatesMock).toHaveBeenLastCalledWith("bot-2");
});

it("recovers from stale session storage and updates on manual selection", async () => {
  sessionStorage.setItem("activeBotId", "missing-bot");

  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-1");
  });

  fireEvent.click(screen.getByText("Paint Two"));

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-2");
  });

  await waitFor(() => {
    expect(sessionStorage.getItem("activeBotId")).toBe("bot-2");
  });
  expect(liveUpdatesMock).toHaveBeenLastCalledWith("bot-2");
});

it("links the logotype back to the main page", async () => {
  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-1");
  });

  expect(screen.getByRole("link", { name: "Go to main page" })).toHaveAttribute(
    "href",
    "/",
  );
});

it("toggles desktop sidebar collapse from the header", async () => {
  const user = userEvent.setup();

  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-collapsed")).toHaveTextContent("false");
  });

  await user.click(screen.getByText("toggle"));

  expect(screen.getByTestId("header-collapsed")).toHaveTextContent("true");
  expect(screen.getByTestId("nav-collapsed")).toHaveTextContent("true");
});

it("opens the mobile drawer from the header toggle and closes it on navigation", async () => {
  const user = userEvent.setup();
  useMediaQueryMock.mockReturnValue(false);

  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-desktop")).toHaveTextContent("false");
  });

  expect(screen.queryByText("navigate")).not.toBeInTheDocument();

  await user.click(screen.getByText("toggle"));
  expect(screen.getByText("navigate")).toBeInTheDocument();

  await user.click(screen.getByText("navigate"));
  await waitFor(() => {
    expect(screen.queryByText("navigate")).not.toBeInTheDocument();
  });
});

it("closes the mobile drawer after selecting a bot from the drawer", async () => {
  const user = userEvent.setup();
  useMediaQueryMock.mockReturnValue(false);

  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-1");
  });

  await user.click(screen.getByText("toggle"));
  await user.click(screen.getAllByText("Paint Two").at(-1)!);

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-2");
  });
  expect(screen.queryByText("navigate")).not.toBeInTheDocument();
});

it("closes a stale mobile drawer when desktop mode becomes active", async () => {
  useMobileNavStore.setState({ isOpen: true });
  renderShell();

  await waitFor(() => {
    expect(screen.queryByText("navigate")).not.toBeInTheDocument();
  });
  expect(useMobileNavStore.getState().isOpen).toBe(false);
});

it("handles an empty bot list without selecting a phantom bot", async () => {
  getBotsMock.mockResolvedValue({ bots: [] });

  renderShell();

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("");
  });
  expect(screen.getByTestId("header-bot-name")).toHaveTextContent("none");
  expect(liveUpdatesMock).toHaveBeenLastCalledWith("");
});

it("does not render the global intro bar on analysis pages", async () => {
  renderShell(["/trades"]);

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-1");
  });

  expect(
    screen.queryByText("Shadow trade history and PnL. Real venue fills stay on Execution."),
  ).not.toBeInTheDocument();
  expect(screen.getByTestId("outlet")).toHaveTextContent("trades");
});

it("pulls the mobile main surface to refresh active dashboard queries", async () => {
  useMediaQueryMock.mockReturnValue(false);
  const { queryClient } = renderShell();
  const invalidateQueries = vi.spyOn(queryClient, "invalidateQueries");
  const refetchQueries = vi.spyOn(queryClient, "refetchQueries");

  await waitFor(() => {
    expect(screen.getByTestId("header-bot-id")).toHaveTextContent("bot-1");
  });

  const main = screen.getByTestId("app-main-scroll");
  Object.defineProperty(main, "scrollTop", {
    configurable: true,
    value: 0,
    writable: true,
  });

  fireEvent.touchStart(main, { touches: [{ clientY: 8 }] });
  fireEvent.touchMove(main, {
    cancelable: true,
    touches: [{ clientY: 180 }],
  });

  expect(screen.getByText("Release to refresh")).toBeInTheDocument();

  fireEvent.touchEnd(main);

  await waitFor(() => {
    expect(invalidateQueries).toHaveBeenCalled();
  });
  expect(refetchQueries).toHaveBeenCalledWith({ type: "active" });
});
