import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Bot } from "../../../lib/types";

const getBotsMock = vi.fn<() => Promise<{ bots: Bot[] }>>();
const liveUpdatesMock = vi.fn<(botId: string) => void>();

vi.mock("../nav", () => ({
  Nav: ({
    bots,
    activeBotId,
    onSelectBot,
  }: {
    bots: Bot[];
    activeBotId: string;
    onSelectBot: (id: string) => void;
  }) => (
    <div data-testid="nav">
      <div data-testid="active-bot">{activeBotId}</div>
      {bots.map((bot) => (
        <button key={bot.id} onClick={() => onSelectBot(bot.id)}>
          {bot.name}
        </button>
      ))}
    </div>
  ),
}));

vi.mock("../header", () => ({
  Header: ({
    bot,
    botId,
    collapsed,
    onToggle,
  }: {
    bot: Bot | null;
    botId: string;
    collapsed: boolean;
    onToggle: () => void;
  }) => (
    <div data-testid="header">
      <span data-testid="header-bot-id">{botId}</span>
      <span data-testid="header-bot-name">{bot?.name ?? "none"}</span>
      <span data-testid="header-collapsed">{String(collapsed)}</span>
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

import { AppShell } from "../app-shell";

function renderShell() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<AppShell />}>
            <Route path="/" element={<div data-testid="outlet">overview</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const bots: Bot[] = [
  { id: "bot-1", name: "Paint One" },
  { id: "bot-2", name: "Paint Two" },
];

beforeEach(() => {
  sessionStorage.clear();
  getBotsMock.mockReset();
  liveUpdatesMock.mockReset();
  getBotsMock.mockResolvedValue({ bots });
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
