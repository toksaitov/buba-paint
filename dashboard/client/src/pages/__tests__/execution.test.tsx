import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint", bot: { id: "paint", name: "Paint" } }),
}));

vi.mock("../../hooks/use-trading-summary", () => ({
  useTradingSummary: vi.fn(),
}));

vi.mock("../../hooks/use-live-status", () => ({
  useLiveSessions: vi.fn(),
  useLiveOrders: vi.fn(),
  useLiveFills: vi.fn(),
  useLiveRedemptions: vi.fn(),
  useLiveReconciliation: vi.fn(),
  useLiveControlAudit: vi.fn(),
}));

vi.mock("../../lib/api", () => ({
  sendLiveControl: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { ExecutionPage } from "../execution";
import {
  useLiveControlAudit,
  useLiveFills,
  useLiveOrders,
  useLiveReconciliation,
  useLiveRedemptions,
  useLiveSessions,
} from "../../hooks/use-live-status";
import { useTradingSummary } from "../../hooks/use-trading-summary";
import { sendLiveControl } from "../../lib/api";
import { useAuthStore } from "../../stores/auth-store";
import type { TradingSummary } from "../../lib/types";

const mockUseTradingSummary = vi.mocked(useTradingSummary);
const mockSendLiveControl = vi.mocked(sendLiveControl);
const mockUseLiveControlAudit = vi.mocked(useLiveControlAudit);
const mockUseLiveSessions = vi.mocked(useLiveSessions);
const mockUseLiveOrders = vi.mocked(useLiveOrders);
const mockUseLiveFills = vi.mocked(useLiveFills);
const mockUseLiveRedemptions = vi.mocked(useLiveRedemptions);
const mockUseLiveReconciliation = vi.mocked(useLiveReconciliation);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: qc }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useAuthStore.setState({ token: "token", user: { id: "1", username: "admin", role: "admin" } });
  mockSendLiveControl.mockResolvedValue({
    ok: true,
    command_id: 123,
    action: "preflight",
    status: "pending",
  });
  mockUseLiveSessions.mockReturnValue({ data: { sessions: [] } } as ReturnType<typeof useLiveSessions>);
  mockUseLiveOrders.mockReturnValue({ data: { orders: [] } } as ReturnType<typeof useLiveOrders>);
  mockUseLiveFills.mockReturnValue({ data: { fills: [] } } as ReturnType<typeof useLiveFills>);
  mockUseLiveRedemptions.mockReturnValue({ data: { redemptions: [] } } as ReturnType<typeof useLiveRedemptions>);
  mockUseLiveReconciliation.mockReturnValue({ data: { events: [] } } as ReturnType<typeof useLiveReconciliation>);
  mockUseLiveControlAudit.mockReturnValue({ data: { entries: [] } } as ReturnType<typeof useLiveControlAudit>);
});

function baseTradingSummary(overrides: Partial<TradingSummary> = {}): TradingSummary {
  return {
    runtime_mode: "live_trading",
    trading_state: "disarmed",
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
      win_rate: 0.66,
      open_trades: 0,
      uptime_hours: 1,
      high_water_mark: 125,
      max_drawdown_pct: 0.1,
      live_session_status: "disarmed",
      last_tick_at: null,
      current_window: null,
    },
    real_account_summary: {
      available_cash: 96,
      reserved_cash: 0,
      inventory_mark_value: 2,
      redeemable_value: 0,
      pending_redeem_value: 0,
      total_equity: 99,
      allowance_available: 96,
      latest_snapshot_at_ms: 1100,
      session_id: 1,
      session_status: "disarmed",
      session_started_at_ms: 1000,
      wallet_address: "0xwallet",
      proxy_wallet: "0xproxy",
      cash_cap_usd: 100,
      enabled_strategies: ["latency-arb"],
      provider: "polymarket",
      user_stream_status: "ok",
      last_user_stream_connected_at_ms: Date.now(),
      last_user_stream_event_at_ms: Date.now(),
      last_account_refresh_at_ms: Date.now(),
      open_orders: 0,
      pending_redemptions: 0,
      critical_reconciliation_events: 0,
    },
    capabilities: {
      preflight: { enabled: true, reason: "Queue an audited preflight refresh." },
      arm: { enabled: true, reason: "All system gates are green." },
      disarm: { enabled: false, reason: "Not armed." },
      cancel_all: { enabled: false, reason: "No open venue orders." },
      stop_after_flat: { enabled: false, reason: "Not armed." },
      redeem: { enabled: false, reason: "No redeemable positions." },
      kill_switch: { enabled: true, reason: "Halt this session." },
    },
    alerts: [],
    ...overrides,
  };
}

function mockExecutionSummary(summary: TradingSummary) {
  mockUseTradingSummary.mockReturnValue({
    isLoading: false,
    data: summary,
  } as ReturnType<typeof useTradingSummary>);
}

function mockLatestSession() {
  mockUseLiveSessions.mockReturnValue({
    data: {
      sessions: [
        {
          id: 1,
          started_at_ms: 1000,
          ended_at_ms: null,
          status: "disarmed",
          execution_mode: "live_trading",
          wallet_address: "0xwallet",
          proxy_wallet: "0xproxy",
          enabled_strategies_json: "[\"latency-arb\"]",
          config_fingerprint: "fingerprint-value",
          cash_cap_usd: 100,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveSessions>);
}

function getActionRow(action: string): HTMLElement {
  const row = document.querySelector(`[data-action="${action}"]`);
  if (!row) throw new Error(`Could not find row for action "${action}"`);
  return row as HTMLElement;
}

function getReasonField(action: string) {
  return within(getActionRow(action)).getByRole("textbox", { name: /reason/i });
}

function getConfirmationField(action: string) {
  return within(getActionRow(action)).getByRole("textbox", { name: /confirmation phrase/i });
}

function getActionButton(action: string, label: RegExp | string) {
  return within(getActionRow(action)).getByRole("button", { name: label });
}

test("shows loading state", () => {
  mockUseTradingSummary.mockReturnValue({
    isLoading: true,
    data: undefined,
  } as ReturnType<typeof useTradingSummary>);
  render(<ExecutionPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders the execution cockpit", () => {
  mockExecutionSummary(
    baseTradingSummary({
      runtime_mode: "live_readonly",
      trading_state: "readonly",
      capabilities: {
        preflight: { enabled: false, reason: "disabled" },
        arm: { enabled: false, reason: "disabled" },
        disarm: { enabled: false, reason: "disabled" },
        cancel_all: { enabled: false, reason: "disabled" },
        stop_after_flat: { enabled: false, reason: "disabled" },
        redeem: { enabled: false, reason: "disabled" },
        kill_switch: { enabled: false, reason: "disabled" },
      },
    }),
  );
  mockUseLiveSessions.mockReturnValue({
    data: {
      sessions: [
        {
          id: 1,
          started_at_ms: 1000,
          ended_at_ms: null,
          status: "readonly_ready",
          execution_mode: "live_readonly",
          wallet_address: "0xwallet",
          proxy_wallet: "0xproxy",
          enabled_strategies_json: "[\"latency-arb\"]",
          config_fingerprint: "fp",
          cash_cap_usd: 100,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveSessions>);

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Runtime Status")).toBeInTheDocument();
  expect(screen.getByText("Controls")).toBeInTheDocument();
  expect(screen.getByText("Polymarket Account")).toBeInTheDocument();
  expect(screen.getByText("Venue Activity")).toBeInTheDocument();
  expect(screen.getByText("Session and Wallet")).toBeInTheDocument();
  expect(screen.getAllByText("Live Readonly").length).toBeGreaterThan(0);
});

test("paper mode renders the control deck as a read-only preview", () => {
  mockExecutionSummary(
    baseTradingSummary({
      runtime_mode: "paper",
      trading_state: "paper",
      real_account_summary: {
        ...baseTradingSummary().real_account_summary,
        latest_snapshot_at_ms: null,
        session_id: null,
        session_status: null,
        session_started_at_ms: null,
        provider: null,
        user_stream_status: null,
        last_user_stream_connected_at_ms: null,
        last_user_stream_event_at_ms: null,
        last_account_refresh_at_ms: null,
      },
      capabilities: {
        preflight: { enabled: false, reason: "Paper mode." },
        arm: { enabled: false, reason: "Paper mode." },
        disarm: { enabled: false, reason: "Paper mode." },
        cancel_all: { enabled: false, reason: "Paper mode." },
        stop_after_flat: { enabled: false, reason: "Paper mode." },
        redeem: { enabled: false, reason: "Paper mode." },
        kill_switch: { enabled: false, reason: "Paper mode." },
      },
    }),
  );

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Paper Mode")).toBeInTheDocument();
  expect(screen.getByText("Controls")).toBeInTheDocument();
  expect(screen.queryByText(/Live controls are inactive/i)).not.toBeInTheDocument();
  expect(getActionButton("preflight", /^Preflight$/)).toBeDisabled();
  expect(getActionButton("arm", /^Arm$/)).toBeDisabled();
  expect(getActionButton("disarm", /^Disarm$/)).toBeDisabled();
  expect(getActionButton("stop_after_flat", /^Stop After Flat$/)).toBeDisabled();
  expect(getActionButton("cancel_all", /^Cancel All$/)).toBeDisabled();
  expect(getActionButton("redeem_all", /^Redeem All$/)).toBeDisabled();
  expect(getActionButton("kill_switch", /^Kill Switch$/)).toBeDisabled();
});

test("queues an audited preflight command from live trading controls", async () => {
  const user = userEvent.setup();
  mockExecutionSummary(baseTradingSummary());
  mockLatestSession();

  render(<ExecutionPage />, { wrapper: createWrapper() });

  await user.type(getReasonField("preflight"), "manual gate refresh");
  await user.click(getActionButton("preflight", /^Preflight$/));

  expect(mockSendLiveControl).toHaveBeenCalledWith("paint", {
    action: "preflight",
    reason: "manual gate refresh",
    confirmation: undefined,
  });
  expect(await screen.findByText(/Command #123 queued as pending/)).toBeInTheDocument();
});

test("limits disarmed controls to preflight and arm", () => {
  mockExecutionSummary(baseTradingSummary());
  mockLatestSession();

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByRole("button", { name: "Preflight" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Arm" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Kill Switch" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Cancel All" })).toBeNull();
});

test("blocks live-control mutations for observers", () => {
  useAuthStore.setState({
    token: "token",
    user: { id: "2", username: "observer", role: "observer" },
  });
  mockExecutionSummary(baseTradingSummary());

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getAllByText("Admin role required.").length).toBeGreaterThan(0);
  expect(getActionButton("preflight", /^Preflight$/)).toBeDisabled();
  expect(mockSendLiveControl).not.toHaveBeenCalled();
});

test("does not expose arm controls after a live session halt", () => {
  mockExecutionSummary(
    baseTradingSummary({
      trading_state: "halted",
      real_account_summary: {
        ...baseTradingSummary().real_account_summary,
        session_status: "halted",
      },
      risk_summary: {
        halt_reason: "daily loss limit",
        halt_at_ms: 1500,
        high_water_mark: 105,
        trough_equity: 80,
        current_equity: 85,
        daily_loss_usd: -15,
        session_drawdown_usd: -20,
        closeout_exported_at_ms: null,
      },
      capabilities: {
        ...baseTradingSummary().capabilities,
        preflight: { enabled: true, reason: "Queue an audited preflight refresh." },
        arm: { enabled: false, reason: "Session halted." },
        kill_switch: { enabled: false, reason: "Already halted." },
      },
    }),
  );

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Session Halted")).toBeInTheDocument();
  expect(screen.getByText("not exported")).toBeInTheDocument();
  expect(getActionButton("cancel_all", /^Cancel All$/)).toBeDisabled();
  expect(getActionButton("redeem_all", /^Redeem All$/)).toBeDisabled();
  expect(screen.queryByRole("button", { name: "Arm" })).toBeNull();
  expect(screen.getByText(/Recovery Actions/)).toBeInTheDocument();
});

test("keeps unknown-order controls focused on reconciliation actions", () => {
  mockExecutionSummary(
    baseTradingSummary({
      trading_state: "unknown_order",
      real_account_summary: {
        ...baseTradingSummary().real_account_summary,
        open_orders: 1,
        session_status: "unknown_order",
      },
      capabilities: {
        ...baseTradingSummary().capabilities,
        arm: { enabled: false, reason: "Order state is unknown." },
        cancel_all: { enabled: true, reason: "Cancel tracked venue orders." },
      },
    }),
  );

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Unknown Order State")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Preflight" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel All" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Kill Switch" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Arm" })).toBeNull();
});

test("blocks arming from mobile even with valid reason and confirmation", async () => {
  const user = userEvent.setup();
  const originalMatchMedia = window.matchMedia;
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockReturnValue({
      matches: true,
      media: "(max-width: 767px)",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });
  mockExecutionSummary(baseTradingSummary());
  mockLatestSession();

  render(<ExecutionPage />, { wrapper: createWrapper() });

  await user.type(getReasonField("arm"), "ready from phone");
  await user.type(getConfirmationField("arm"), 'ARM "paint" fingerprint-v');

  expect(screen.getByText("Desktop confirmation required.")).toBeInTheDocument();
  expect(getActionButton("arm", /^Arm$/)).toBeDisabled();
  expect(mockSendLiveControl).not.toHaveBeenCalled();

  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: originalMatchMedia,
  });
});

test("renders cash-headroom progress bar when a cap is set", () => {
  mockExecutionSummary(
    baseTradingSummary({
      real_account_summary: {
        ...baseTradingSummary().real_account_summary,
        reserved_cash: 60,
        inventory_mark_value: 30,
        cash_cap_usd: 100,
      },
    }),
  );
  mockLatestSession();

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText(/Cash in Play/)).toBeInTheDocument();
  expect(screen.getByText(/of \$100\.00 \(90\.0%\)/)).toBeInTheDocument();
  expect(screen.getByText(/\$10\.00 available/)).toBeInTheDocument();
});

test("renders recent audit strip with parsed details", () => {
  mockExecutionSummary(baseTradingSummary());
  mockLatestSession();
  mockUseLiveControlAudit.mockReturnValue({
    isLoading: false,
    data: {
      entries: [
        {
          id: 7,
          timestamp_ms: Date.now() - 5_000,
          actor: "alice@host",
          action: "preflight",
          target: "control audit",
          details_json: JSON.stringify({
            reason: "manual gate refresh",
            status: "applied",
            command: "preflight",
          }),
        },
        {
          id: 6,
          timestamp_ms: Date.now() - 60_000,
          actor: "bob@host",
          action: "disarm",
          target: null,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveControlAudit>);

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Recent Commands")).toBeInTheDocument();
  expect(screen.getByText("alice@host")).toBeInTheDocument();
  expect(screen.getByText("bob@host")).toBeInTheDocument();
});

test("scopes per-action errors to the action that failed", async () => {
  const user = userEvent.setup();
  mockSendLiveControl.mockRejectedValueOnce(new Error("preflight rejected by gate"));
  mockExecutionSummary(baseTradingSummary());
  mockLatestSession();

  render(<ExecutionPage />, { wrapper: createWrapper() });

  await user.type(getReasonField("preflight"), "manual gate refresh");
  await user.click(getActionButton("preflight", /^Preflight$/));

  await waitFor(() => {
    expect(screen.getByText("Command Failed")).toBeInTheDocument();
  });
  const row = getActionRow("preflight");
  expect(within(row).getByText(/preflight rejected by gate/)).toBeInTheDocument();
});

test("surfaces a stale-data banner when user stream is stalled", () => {
  mockExecutionSummary(
    baseTradingSummary({
      real_account_summary: {
        ...baseTradingSummary().real_account_summary,
        user_stream_status: "stalled",
      },
    }),
  );

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Live State May Be Stale")).toBeInTheDocument();
  expect(screen.getByText(/user stream stalled/)).toBeInTheDocument();
});

test("renders non-empty trading activity, alerts, and advanced details", async () => {
  const user = userEvent.setup();
  mockExecutionSummary(
    baseTradingSummary({
      trading_state: "degraded",
      venue_health: { state: "warning", label: "User stream degraded", detail: "stale" },
      account_health: { state: "warning", label: "Allowance unknown", detail: "missing" },
      reconciliation_health: {
        state: "critical",
        label: "Critical divergence",
        detail: "remote mismatch",
      },
      shadow_summary: {
        ...baseTradingSummary().shadow_summary,
        balance: 90,
        starting_balance: 100,
        total_pnl: -10,
        total_trades: 5,
        wins: 2,
        losses: 3,
        win_rate: 0.4,
        open_trades: 1,
        uptime_hours: 5,
        high_water_mark: 110,
        max_drawdown_pct: 0.2,
        live_session_status: "readonly_degraded",
      },
      real_account_summary: {
        ...baseTradingSummary().real_account_summary,
        available_cash: 75,
        reserved_cash: 5,
        inventory_mark_value: 10,
        redeemable_value: 3,
        pending_redeem_value: 2,
        total_equity: 90,
        allowance_available: null,
        cash_cap_usd: 100,
        wallet_address: "0x1234567890abcdef1234567890abcdef12345678",
        proxy_wallet: "0xabcdef1234567890abcdef1234567890abcdef12",
        enabled_strategies: ["latency-arb", "spread-capture"],
        provider: "polymarket",
        user_stream_status: "ok",
        open_orders: 1,
        pending_redemptions: 1,
        critical_reconciliation_events: 1,
      },
      capabilities: {
        preflight: { enabled: true, reason: "ready" },
        arm: { enabled: false, reason: "gated" },
        disarm: { enabled: false, reason: "gated" },
        cancel_all: { enabled: false, reason: "gated" },
        stop_after_flat: { enabled: false, reason: "gated" },
        redeem: { enabled: false, reason: "gated" },
        kill_switch: { enabled: false, reason: "gated" },
      },
      alerts: [
        { severity: "warning", title: "Venue lag", detail: "User stream is stale." },
        { severity: "critical", title: "Critical recon", detail: "Remote mismatch." },
      ],
    }),
  );
  mockUseLiveSessions.mockReturnValue({
    data: {
      sessions: [
        {
          id: 2,
          started_at_ms: 2000,
          ended_at_ms: null,
          status: "readonly_degraded",
          execution_mode: "live_readonly",
          wallet_address: "0xwallet",
          proxy_wallet: "0xproxy",
          enabled_strategies_json: "[\"latency-arb\",\"spread-capture\"]",
          config_fingerprint: "fingerprint-value-1234567890",
          cash_cap_usd: 100,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveSessions>);
  mockUseLiveOrders.mockReturnValue({
    data: {
      orders: [
        {
          id: 10,
          session_id: 2,
          intent_id: 1,
          venue_order_id: "ord-1",
          client_order_id: "client-1",
          market_id: "mkt-1",
          token_id: "tok-1",
          side: "buy",
          order_type: "fok",
          status: "rejected",
          status_reason: null,
          created_at_ms: 2400,
          acknowledged_at_ms: null,
          updated_at_ms: 2450,
          requested_price: 0.55,
          limit_price: 0.55,
          requested_size: 20,
          accepted_size: null,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveOrders>);
  mockUseLiveFills.mockReturnValue({
    data: {
      fills: [
        {
          id: 20,
          session_id: 2,
          intent_id: 1,
          live_order_id: 10,
          venue_trade_id: "fill-1",
          filled_at_ms: 2500,
          price: 0.56,
          size: 12,
          fee_amount: null,
          fee_rate: null,
          liquidity_side: null,
          tx_hash: null,
          status: "filled",
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveFills>);
  mockUseLiveRedemptions.mockReturnValue({
    data: {
      redemptions: [
        {
          id: 30,
          session_id: 2,
          market_id: "mkt-1",
          detected_redeemable_at_ms: 2600,
          submitted_at_ms: null,
          confirmed_at_ms: null,
          cash_credit_observed_at_ms: null,
          status: "pending",
          redeemable_value: 3,
          tx_hash: null,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveRedemptions>);
  mockUseLiveReconciliation.mockReturnValue({
    data: {
      events: [
        {
          id: 40,
          session_id: 2,
          timestamp_ms: 2700,
          severity: "critical",
          event_type: "remote_open_orders",
          local_value: null,
          remote_value: null,
          details_json: null,
        },
      ],
    },
  } as ReturnType<typeof useLiveReconciliation>);

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getAllByText("Critical divergence").length).toBeGreaterThan(0);
  expect(screen.getByText("Venue lag")).toBeInTheDocument();
  expect(screen.getByText("rejected")).toBeInTheDocument();
  expect(screen.getByText("filled")).toBeInTheDocument();
  expect(screen.getByText("pending")).toBeInTheDocument();
  expect(screen.getByText("remote_open_orders")).toBeInTheDocument();

  await user.click(screen.getByText("Session and Wallet"));
  expect(screen.getAllByText(/fingerprint/).length).toBeGreaterThan(1);
});

test("shows degraded Execution detail panels when live detail queries fail", async () => {
  const user = userEvent.setup();
  mockExecutionSummary(
    baseTradingSummary({
      runtime_mode: "live_readonly",
      trading_state: "readonly",
      capabilities: {
        preflight: { enabled: false, reason: "disabled" },
        arm: { enabled: false, reason: "disabled" },
        disarm: { enabled: false, reason: "disabled" },
        cancel_all: { enabled: false, reason: "disabled" },
        stop_after_flat: { enabled: false, reason: "disabled" },
        redeem: { enabled: false, reason: "disabled" },
        kill_switch: { enabled: false, reason: "disabled" },
      },
    }),
  );
  mockUseLiveSessions.mockReturnValue({
    isError: true,
    isLoading: false,
    error: new Error("session endpoint failed"),
    data: undefined,
  } as ReturnType<typeof useLiveSessions>);
  mockUseLiveOrders.mockReturnValue({
    isError: true,
    isLoading: false,
    error: new Error("orders endpoint failed"),
    data: undefined,
  } as ReturnType<typeof useLiveOrders>);
  mockUseLiveFills.mockReturnValue({
    isError: false,
    isLoading: false,
    data: { fills: [] },
  } as ReturnType<typeof useLiveFills>);
  mockUseLiveRedemptions.mockReturnValue({
    isError: true,
    isLoading: false,
    error: new Error("redemptions endpoint failed"),
    data: undefined,
  } as ReturnType<typeof useLiveRedemptions>);
  mockUseLiveReconciliation.mockReturnValue({
    isError: false,
    isLoading: false,
    data: { events: [] },
  } as ReturnType<typeof useLiveReconciliation>);

  render(<ExecutionPage />, { wrapper: createWrapper() });

  expect(screen.getByText("Detail Panels Degraded")).toBeInTheDocument();
  expect(
    screen.getByText(
      /Summary is current, but some execution detail panels are unavailable:\s*Session details, Orders, Redemptions\./,
    ),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/Venue orders are currently unavailable: orders endpoint failed/),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/Redemption details are currently unavailable: redemptions endpoint failed/),
  ).toBeInTheDocument();
  expect(screen.getByText("No venue fills recorded.")).toBeInTheDocument();
  expect(screen.getByText("No reconciliation events recorded.")).toBeInTheDocument();

  await user.click(screen.getByText("Session and Wallet"));
  expect(
    screen.getByText(/Live session details are currently unavailable: session endpoint failed/),
  ).toBeInTheDocument();
});
