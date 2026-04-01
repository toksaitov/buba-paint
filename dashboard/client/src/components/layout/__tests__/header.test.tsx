import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { Header } from "../header";
import { renderWithProviders } from "../../../test/test-utils";
import { useBotStatus } from "../../../hooks/use-bot-status";
import { useAuthStore } from "../../../stores/auth-store";

vi.mock("../../../lib/api", () => ({
  getBotProcessStatus: vi.fn(),
  botStart: vi.fn(),
  botStop: vi.fn(),
  botRestart: vi.fn(),
}));

vi.mock("../../../hooks/use-bot-status", () => ({
  useBotStatus: vi.fn(),
}));

import {
  getBotProcessStatus,
  botStart,
  botStop,
  botRestart,
} from "../../../lib/api";

const mockGetProcessStatus = vi.mocked(getBotProcessStatus);
const mockBotStart = vi.mocked(botStart);
const mockBotStop = vi.mocked(botStop);
const mockBotRestart = vi.mocked(botRestart);
const mockUseBotStatus = vi.mocked(useBotStatus);

beforeEach(() => {
  useAuthStore.setState({
    token: "test-token",
    user: { id: "1", username: "admin", role: "admin" },
  });
  mockGetProcessStatus.mockResolvedValue({
    active: true,
    pid: 42,
    uptime_secs: 120,
    control_available: true,
  });
  mockUseBotStatus.mockReturnValue({
    data: {
      balance: 200,
      starting_balance: 200,
      total_trades: 0,
      wins: 0,
      losses: 0,
      win_rate: 0,
      total_pnl: 0,
      max_drawdown_pct: 0,
      high_water_mark: 200,
      uptime_hours: 1,
      open_trades: 0,
      current_window: null,
      last_tick_at: null,
    },
  } as ReturnType<typeof useBotStatus>);
  mockBotStart.mockResolvedValue({
    active: true,
    pid: 43,
    uptime_secs: 0,
    control_available: true,
  });
  mockBotStop.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
    control_available: true,
  });
  mockBotRestart.mockResolvedValue({
    active: true,
    pid: 44,
    uptime_secs: 0,
    control_available: true,
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

function renderHeader() {
  return renderWithProviders(
    <Header
      bot={{ id: "bot-1", name: "Test Bot" }}
      botId="bot-1"
      collapsed={false}
      onToggle={() => {}}
    />,
  );
}

test("shows Running badge when process is active", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
});

test("shows Stopped badge when process is inactive", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
    control_available: true,
  });
  renderHeader();
  await waitFor(() => expect(screen.getByText("Stopped")).toBeInTheDocument());
});

test("shows Running badge in monitor-only mode when ticks are fresh", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
    control_available: false,
  });
  mockUseBotStatus.mockReturnValue({
    data: {
      balance: 200,
      starting_balance: 200,
      total_trades: 0,
      wins: 0,
      losses: 0,
      win_rate: 0,
      total_pnl: 0,
      max_drawdown_pct: 0,
      high_water_mark: 200,
      uptime_hours: 1,
      open_trades: 0,
      current_window: null,
      last_tick_at: Date.now() - 5_000,
    },
  } as ReturnType<typeof useBotStatus>);
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
});

test("Start button disabled when process is running", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  expect(screen.getByTitle("Bot is already running")).toBeDisabled();
});

test("Stop button disabled when process is stopped", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
    control_available: true,
  });
  renderHeader();
  await waitFor(() => expect(screen.getByText("Stopped")).toBeInTheDocument());
  const disabledButtons = screen.getAllByTitle("Bot is not running");
  expect(disabledButtons.length).toBe(2);
  disabledButtons.forEach((btn) => expect(btn).toBeDisabled());
});

test("Start button enabled when process is stopped", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
    control_available: true,
  });
  renderHeader();
  await waitFor(() => expect(screen.getByText("Stopped")).toBeInTheDocument());
  expect(screen.getByTitle("Start bot")).toBeEnabled();
});

test("disables controls in monitor-only mode", async () => {
  mockGetProcessStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
    control_available: false,
  });
  mockUseBotStatus.mockReturnValue({
    data: {
      balance: 200,
      starting_balance: 200,
      total_trades: 0,
      wins: 0,
      losses: 0,
      win_rate: 0,
      total_pnl: 0,
      max_drawdown_pct: 0,
      high_water_mark: 200,
      uptime_hours: 1,
      open_trades: 0,
      current_window: null,
      last_tick_at: Date.now() - 5_000,
    },
  } as ReturnType<typeof useBotStatus>);
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  const disabledButtons = screen.getAllByTitle(
    "Process control unavailable in monitor-only mode",
  );
  expect(disabledButtons.length).toBe(3);
  disabledButtons.forEach((btn) => expect(btn).toBeDisabled());
});

test("Stop button enabled when process is running", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  expect(screen.getByTitle("Stop bot")).toBeEnabled();
});

test("Stop button calls botStop", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() => expect(mockBotStop).toHaveBeenCalledWith("bot-1"));
});

test("shows error toast on action failure", async () => {
  mockBotStop.mockRejectedValue(new Error("something broke"));
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() =>
    expect(screen.getByText("something broke")).toBeInTheDocument(),
  );
});

test("error toast dismisses on X click", async () => {
  mockBotStop.mockRejectedValue(new Error("test error"));
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() =>
    expect(screen.getByText("test error")).toBeInTheDocument(),
  );
  const errorBanner = screen.getByText("test error").closest("div")!;
  const dismissBtn = errorBanner.querySelector("button")!;
  await userEvent.click(dismissBtn);
  expect(screen.queryByText("test error")).not.toBeInTheDocument();
});

test("shows uptime when process is running", async () => {
  renderHeader();
  await waitFor(() => expect(screen.getByText("Running")).toBeInTheDocument());
  expect(screen.getByText("2m")).toBeInTheDocument();
});

test("displays bot name", () => {
  renderHeader();
  expect(screen.getByText("Test Bot")).toBeInTheDocument();
});
