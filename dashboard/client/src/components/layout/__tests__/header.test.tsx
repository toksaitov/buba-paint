import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { Header } from "../header";
import { renderWithProviders } from "../../../test/test-utils";
import { useBotStatus } from "../../../hooks/use-bot-status";
import { useProcessStatus } from "../../../hooks/use-process-status";
import { useAuthStore } from "../../../stores/auth-store";
import { useThemeStore } from "../../../stores/theme-store";

vi.mock("../../../lib/api", () => ({
  botStart: vi.fn(),
  botStop: vi.fn(),
  botRestart: vi.fn(),
}));

vi.mock("../../../hooks/use-bot-status", () => ({
  useBotStatus: vi.fn(),
}));

vi.mock("../../../hooks/use-process-status", () => ({
  useProcessStatus: vi.fn(),
}));

vi.mock("../../../lib/notifications", () => ({
  isNotificationSupported: vi.fn(),
  isNotificationEnabled: vi.fn(),
  setNotificationEnabled: vi.fn(),
  requestNotificationPermission: vi.fn(),
}));

import {
  botStart,
  botStop,
  botRestart,
} from "../../../lib/api";
import {
  isNotificationSupported,
  isNotificationEnabled,
  setNotificationEnabled,
  requestNotificationPermission,
} from "../../../lib/notifications";

const mockBotStart = vi.mocked(botStart);
const mockBotStop = vi.mocked(botStop);
const mockBotRestart = vi.mocked(botRestart);
const mockUseBotStatus = vi.mocked(useBotStatus);
const mockUseProcessStatus = vi.mocked(useProcessStatus);
const mockIsNotificationSupported = vi.mocked(isNotificationSupported);
const mockIsNotificationEnabled = vi.mocked(isNotificationEnabled);
const mockSetNotificationEnabled = vi.mocked(setNotificationEnabled);
const mockRequestNotificationPermission = vi.mocked(requestNotificationPermission);

beforeEach(() => {
  localStorage.clear();
  useAuthStore.setState({
    token: "test-token",
    user: { id: "1", username: "admin", role: "admin" },
  });
  useThemeStore.setState({ mode: "system" });
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: true,
      pid: 42,
      uptime_secs: 120,
      control_available: true,
    },
  } as ReturnType<typeof useProcessStatus>);
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
  mockIsNotificationSupported.mockReturnValue(false);
  mockIsNotificationEnabled.mockReturnValue(false);
  mockRequestNotificationPermission.mockResolvedValue(true);
  mockSetNotificationEnabled.mockImplementation(() => {});
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

function renderHeader(
  props: Partial<React.ComponentProps<typeof Header>> = {},
) {
  return renderWithProviders(
    <Header
      bot={{ id: "bot-1", name: "Test Bot" }}
      botId="bot-1"
      collapsed={false}
      onToggle={() => {}}
      isDesktop={true}
      {...props}
    />,
  );
}

test("shows Running badge when process is active", () => {
  renderHeader();
  expect(screen.getByText("Running")).toBeInTheDocument();
});

test("shows Stopped badge when process is inactive", () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: true,
    },
  } as ReturnType<typeof useProcessStatus>);
  renderHeader();
  expect(screen.getByText("Stopped")).toBeInTheDocument();
});

test("shows Running badge in monitor-only mode when ticks are fresh", async () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: false,
    },
  } as ReturnType<typeof useProcessStatus>);
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

test("shows Stopped badge in monitor-only mode when ticks are stale", () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: false,
    },
  } as ReturnType<typeof useProcessStatus>);
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
      last_tick_at: Date.now() - 60_000,
    },
  } as ReturnType<typeof useBotStatus>);
  renderHeader();
  expect(screen.getByText("Stopped")).toBeInTheDocument();
});

test("Start button disabled when process is running", () => {
  renderHeader();
  expect(screen.getByTitle("Bot is already running")).toBeDisabled();
});

test("Stop button disabled when process is stopped", async () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: true,
    },
  } as ReturnType<typeof useProcessStatus>);
  renderHeader();
  const disabledButtons = screen.getAllByTitle("Bot is not running");
  expect(disabledButtons.length).toBe(2);
  disabledButtons.forEach((btn) => expect(btn).toBeDisabled());
});

test("Start button enabled when process is stopped", async () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: true,
    },
  } as ReturnType<typeof useProcessStatus>);
  renderHeader();
  expect(screen.getByTitle("Start bot")).toBeEnabled();
});

test("disables controls in monitor-only mode", async () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: false,
    },
  } as ReturnType<typeof useProcessStatus>);
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
  const disabledButtons = screen.getAllByTitle(
    "Process control unavailable in monitor-only mode",
  );
  expect(disabledButtons.length).toBe(3);
  disabledButtons.forEach((btn) => expect(btn).toBeDisabled());
});

test("Stop button enabled when process is running", async () => {
  renderHeader();
  expect(screen.getByTitle("Stop bot")).toBeEnabled();
});

test("Stop button calls botStop", async () => {
  renderHeader();
  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() => expect(mockBotStop).toHaveBeenCalledWith("bot-1"));
});

test("shows error toast on action failure", async () => {
  mockBotStop.mockRejectedValue(new Error("something broke"));
  renderHeader();
  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() =>
    expect(screen.getByText("something broke")).toBeInTheDocument(),
  );
});

test("error toast dismisses on X click", async () => {
  mockBotStop.mockRejectedValue(new Error("test error"));
  renderHeader();
  await userEvent.click(screen.getByTitle("Stop bot"));
  await waitFor(() =>
    expect(screen.getByText("test error")).toBeInTheDocument(),
  );
  const errorBanner = screen.getByText("test error").closest("div")!;
  const dismissBtn = errorBanner.querySelector("button")!;
  await userEvent.click(dismissBtn);
  expect(screen.queryByText("test error")).not.toBeInTheDocument();
});

test("shows uptime when process is running", () => {
  renderHeader();
  expect(screen.getByText("2m")).toBeInTheDocument();
});

test("displays bot name", () => {
  renderHeader();
  expect(screen.getByText("Test Bot")).toBeInTheDocument();
});

test("uses the mobile navigation toggle label on mobile", () => {
  renderHeader({ isDesktop: false });
  expect(screen.getByTitle("Expand navigation")).toBeInTheDocument();
});

test("cycles theme mode across system, dark, and light", async () => {
  const user = userEvent.setup();
  renderHeader();

  const themeButton = screen.getByTitle("Theme: system");
  await user.click(themeButton);
  expect(screen.getByTitle("Theme: dark")).toBeInTheDocument();

  await user.click(screen.getByTitle("Theme: dark"));
  expect(screen.getByTitle("Theme: light")).toBeInTheDocument();

  await user.click(screen.getByTitle("Theme: light"));
  expect(screen.getByTitle("Theme: system")).toBeInTheDocument();
});

test("hides the notification toggle when notifications are unsupported", () => {
  renderHeader();
  expect(
    screen.queryByTitle(/Enable notifications|Disable notifications/),
  ).not.toBeInTheDocument();
});

test("enables notifications after permission is granted", async () => {
  mockIsNotificationSupported.mockReturnValue(true);
  mockIsNotificationEnabled.mockReturnValue(false);
  const user = userEvent.setup();

  renderHeader();
  await user.click(screen.getByTitle("Enable notifications"));

  await waitFor(() => {
    expect(mockRequestNotificationPermission).toHaveBeenCalled();
    expect(mockSetNotificationEnabled).toHaveBeenCalledWith(true);
  });
  expect(screen.getByTitle("Disable notifications")).toBeInTheDocument();
});

test("disables notifications when already enabled", async () => {
  mockIsNotificationSupported.mockReturnValue(true);
  mockIsNotificationEnabled.mockReturnValue(true);
  const user = userEvent.setup();

  renderHeader();
  await user.click(screen.getByTitle("Disable notifications"));

  expect(mockRequestNotificationPermission).not.toHaveBeenCalled();
  expect(mockSetNotificationEnabled).toHaveBeenCalledWith(false);
  expect(screen.getByTitle("Enable notifications")).toBeInTheDocument();
});
