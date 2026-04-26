import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import { Header } from "../header";
import { renderWithProviders } from "../../../test/test-utils";
import { useProcessStatus } from "../../../hooks/use-process-status";
import { useTradingSummary } from "../../../hooks/use-trading-summary";
import { useAuthStore } from "../../../stores/auth-store";
import { useThemeStore } from "../../../stores/theme-store";

vi.mock("../../../lib/api", () => ({
  botStart: vi.fn(),
  botStop: vi.fn(),
  botRestart: vi.fn(),
}));

vi.mock("../../../hooks/use-process-status", () => ({
  useProcessStatus: vi.fn(),
}));

vi.mock("../../../hooks/use-trading-summary", () => ({
  useTradingSummary: vi.fn(),
}));

vi.mock("../../../lib/notifications", () => ({
  isNotificationSupported: vi.fn(),
  isNotificationEnabled: vi.fn(),
  setNotificationEnabled: vi.fn(),
  requestNotificationPermission: vi.fn(),
}));

import { botRestart, botStart, botStop } from "../../../lib/api";
import {
  isNotificationEnabled,
  isNotificationSupported,
  requestNotificationPermission,
  setNotificationEnabled,
} from "../../../lib/notifications";

const mockBotStart = vi.mocked(botStart);
const mockBotStop = vi.mocked(botStop);
const mockBotRestart = vi.mocked(botRestart);
const mockUseProcessStatus = vi.mocked(useProcessStatus);
const mockUseTradingSummary = vi.mocked(useTradingSummary);
const mockIsNotificationSupported = vi.mocked(isNotificationSupported);
const mockIsNotificationEnabled = vi.mocked(isNotificationEnabled);
const mockSetNotificationEnabled = vi.mocked(setNotificationEnabled);
const mockRequestNotificationPermission = vi.mocked(requestNotificationPermission);

const tradingSummary = {
  runtime_mode: "live_readonly",
  trading_state: "readonly",
  process_state: "running",
  venue_health: { state: "healthy", label: "Venue connected", detail: null },
  account_health: { state: "healthy", label: "Account tracked", detail: null },
  reconciliation_health: {
    state: "healthy",
    label: "Reconciliation clean",
    detail: null,
  },
  shadow_summary: {
    balance: 120,
    starting_balance: 100,
    total_pnl: 20,
    total_trades: 3,
    wins: 2,
    losses: 1,
    win_rate: 2 / 3,
    open_trades: 0,
    uptime_hours: 12,
    high_water_mark: 130,
    max_drawdown_pct: 0.1,
    live_session_status: "readonly_ready",
    last_tick_at: null,
    current_window: null,
  },
  real_account_summary: {
    available_cash: 99,
    reserved_cash: 0,
    inventory_mark_value: 0,
    redeemable_value: 0,
    pending_redeem_value: 0,
    total_equity: 99,
    allowance_available: 99,
    latest_snapshot_at_ms: null,
    session_id: 1,
    session_status: "readonly_ready",
    session_started_at_ms: null,
    wallet_address: null,
    proxy_wallet: null,
    cash_cap_usd: 100,
    enabled_strategies: ["latency-arb"],
    provider: "polymarket",
    user_stream_status: "ok",
    last_user_stream_connected_at_ms: null,
    last_user_stream_event_at_ms: null,
    last_account_refresh_at_ms: null,
    open_orders: 0,
    pending_redemptions: 0,
    critical_reconciliation_events: 0,
  },
  capabilities: {
    preflight: { enabled: false, reason: "disabled" },
    arm: { enabled: false, reason: "disabled" },
    disarm: { enabled: false, reason: "disabled" },
    cancel_all: { enabled: false, reason: "disabled" },
    stop_after_flat: { enabled: false, reason: "disabled" },
    redeem: { enabled: false, reason: "disabled" },
    kill_switch: { enabled: false, reason: "disabled" },
  },
  alerts: [],
};

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
  mockUseTradingSummary.mockReturnValue({
    data: tradingSummary,
  } as ReturnType<typeof useTradingSummary>);
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

function renderHeader(props: Partial<React.ComponentProps<typeof Header>> = {}) {
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

test("shows process state and one meaningful live_readonly badge", () => {
  renderHeader();
  expect(screen.getByText("Running")).toBeInTheDocument();
  expect(screen.getByText("Live Readonly")).toBeInTheDocument();
  expect(screen.queryByText("Readonly")).not.toBeInTheDocument();
});

test("renders paper as a single mobile pill without duplicating desktop badges", () => {
  mockUseTradingSummary.mockReturnValue({
    data: { ...tradingSummary, runtime_mode: "paper", trading_state: "paper" },
  } as ReturnType<typeof useTradingSummary>);

  renderHeader();

  expect(screen.getByText("Running")).toBeInTheDocument();
  expect(screen.getAllByText("Paper")).toHaveLength(1);
});

test("shows stopped process state when process is inactive", () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: true,
    },
  } as ReturnType<typeof useProcessStatus>);
  mockUseTradingSummary.mockReturnValue({
    data: { ...tradingSummary, process_state: "stopped" },
  } as ReturnType<typeof useTradingSummary>);

  renderHeader();
  expect(screen.getByText("Stopped")).toBeInTheDocument();
});

test("shows monitor-only state when control is unavailable", () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: false,
    },
  } as ReturnType<typeof useProcessStatus>);
  mockUseTradingSummary.mockReturnValue({
    data: { ...tradingSummary, process_state: "monitoring" },
  } as ReturnType<typeof useTradingSummary>);

  renderHeader();
  expect(screen.getByText("Monitor Only")).toBeInTheDocument();
});

test("start button is disabled when process is already running", () => {
  renderHeader();
  expect(screen.getByTitle("Bot is already running")).toBeDisabled();
});

test("stop button is disabled when process is not running", () => {
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
  expect(disabledButtons).toHaveLength(2);
});

test("disables process controls in monitor-only mode", () => {
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: false,
    },
  } as ReturnType<typeof useProcessStatus>);
  mockUseTradingSummary.mockReturnValue({
    data: { ...tradingSummary, process_state: "monitoring" },
  } as ReturnType<typeof useTradingSummary>);

  renderHeader();
  screen
    .getAllByTitle("Process control unavailable in monitor-only mode")
    .forEach((button) => expect(button).toBeDisabled());
});

test("runs restart action and invalidates trading summary cache", async () => {
  const user = userEvent.setup();
  renderHeader();

  await user.click(screen.getByTitle("Restart bot"));

  await waitFor(() => {
    expect(mockBotRestart).toHaveBeenCalledWith("bot-1");
  });
});

test("keeps trading alerts out of the header chip row", () => {
  mockUseTradingSummary.mockReturnValue({
    data: {
      ...tradingSummary,
      alerts: [{ severity: "critical", title: "Kill switch", detail: "Trading disarmed." }],
    },
  } as ReturnType<typeof useTradingSummary>);

  renderHeader();
  expect(screen.queryByText("1 Alert")).not.toBeInTheDocument();
  expect(screen.getByText("Live Readonly")).toBeInTheDocument();
});

test("cycles theme mode when the theme button is pressed", async () => {
  const user = userEvent.setup();
  renderHeader();

  await user.click(screen.getByTitle("Theme: system"));
  expect(useThemeStore.getState().mode).toBe("dark");
});

test("enables notifications when the browser permission is granted", async () => {
  const user = userEvent.setup();
  mockIsNotificationSupported.mockReturnValue(true);
  mockIsNotificationEnabled.mockReturnValue(false);

  renderHeader();

  await user.click(screen.getByTitle("Enable notifications"));

  expect(mockRequestNotificationPermission).toHaveBeenCalled();
  expect(mockSetNotificationEnabled).toHaveBeenCalledWith(true);
});

test("surfaces action errors when a process control call fails", async () => {
  const user = userEvent.setup();
  mockUseProcessStatus.mockReturnValue({
    data: {
      active: false,
      pid: null,
      uptime_secs: null,
      control_available: true,
    },
  } as ReturnType<typeof useProcessStatus>);
  mockUseTradingSummary.mockReturnValue({
    data: { ...tradingSummary, process_state: "stopped" },
  } as ReturnType<typeof useTradingSummary>);
  mockBotStart.mockRejectedValue(new Error("start failed"));

  renderHeader();

  await user.click(screen.getByTitle("Start bot"));

  await waitFor(() => {
    expect(screen.getByText("start failed")).toBeInTheDocument();
  });
});
