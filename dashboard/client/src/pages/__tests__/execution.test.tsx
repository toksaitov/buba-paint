import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
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
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { ExecutionPage } from "../execution";
import {
  useLiveFills,
  useLiveOrders,
  useLiveReconciliation,
  useLiveRedemptions,
  useLiveSessions,
} from "../../hooks/use-live-status";
import { useTradingSummary } from "../../hooks/use-trading-summary";

const mockUseTradingSummary = vi.mocked(useTradingSummary);
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
  mockUseLiveSessions.mockReturnValue({ data: { sessions: [] } } as ReturnType<typeof useLiveSessions>);
  mockUseLiveOrders.mockReturnValue({ data: { orders: [] } } as ReturnType<typeof useLiveOrders>);
  mockUseLiveFills.mockReturnValue({ data: { fills: [] } } as ReturnType<typeof useLiveFills>);
  mockUseLiveRedemptions.mockReturnValue({ data: { redemptions: [] } } as ReturnType<typeof useLiveRedemptions>);
  mockUseLiveReconciliation.mockReturnValue({ data: { events: [] } } as ReturnType<typeof useLiveReconciliation>);
});

test("shows loading state", () => {
  mockUseTradingSummary.mockReturnValue({
    isLoading: true,
    data: undefined,
  } as ReturnType<typeof useTradingSummary>);
  render(<ExecutionPage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeInTheDocument();
});

test("renders the execution cockpit", () => {
  mockUseTradingSummary.mockReturnValue({
    isLoading: false,
    data: {
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
        win_rate: 0.66,
        open_trades: 0,
        uptime_hours: 1,
        high_water_mark: 125,
        max_drawdown_pct: 0.1,
        live_session_status: "readonly_ready",
        last_tick_at: null,
        current_window: null,
      },
      real_account_summary: {
        available_cash: 96,
        reserved_cash: 0,
        inventory_mark_value: 2,
        redeemable_value: 1,
        pending_redeem_value: 0,
        total_equity: 99,
        allowance_available: 96,
        latest_snapshot_at_ms: 1100,
        session_id: 1,
        session_status: "readonly_ready",
        session_started_at_ms: 1000,
        wallet_address: "0xwallet",
        proxy_wallet: "0xproxy",
        cash_cap_usd: 100,
        enabled_strategies: ["latency-arb"],
        provider: "polymarket",
        user_stream_status: "ok",
        last_user_stream_connected_at_ms: 1200,
        last_user_stream_event_at_ms: 1250,
        last_account_refresh_at_ms: 1300,
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
    },
  } as ReturnType<typeof useTradingSummary>);
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

  expect(screen.getByText("Runtime status")).toBeInTheDocument();
  expect(screen.getByText("Controls")).toBeInTheDocument();
  expect(screen.getByText("Polymarket account")).toBeInTheDocument();
  expect(screen.getByText("Venue activity")).toBeInTheDocument();
  expect(screen.getByText("Session and wallet")).toBeInTheDocument();
  expect(screen.getByText("Live Readonly")).toBeInTheDocument();
});

test("renders non-empty trading activity, alerts, and advanced details", async () => {
  const user = userEvent.setup();
  mockUseTradingSummary.mockReturnValue({
    isLoading: false,
    data: {
      runtime_mode: "live_trading",
      trading_state: "degraded",
      process_state: "running",
      venue_health: { state: "warning", label: "User stream degraded", detail: "stale" },
      account_health: { state: "warning", label: "Allowance unknown", detail: "missing" },
      reconciliation_health: {
        state: "critical",
        label: "Critical divergence",
        detail: "remote mismatch",
      },
      shadow_summary: {
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
        last_tick_at: null,
        current_window: null,
      },
      real_account_summary: {
        available_cash: 75,
        reserved_cash: 5,
        inventory_mark_value: 10,
        redeemable_value: 3,
        pending_redeem_value: 2,
        total_equity: 90,
        allowance_available: null,
        latest_snapshot_at_ms: 2100,
        session_id: 2,
        session_status: "readonly_degraded",
        session_started_at_ms: 2000,
        wallet_address: "0x1234567890abcdef1234567890abcdef12345678",
        proxy_wallet: "0xabcdef1234567890abcdef1234567890abcdef12",
        cash_cap_usd: 100,
        enabled_strategies: ["latency-arb", "spread-capture"],
        provider: "polymarket",
        user_stream_status: "lagged",
        last_user_stream_connected_at_ms: 2200,
        last_user_stream_event_at_ms: 2250,
        last_account_refresh_at_ms: 2300,
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
    },
  } as ReturnType<typeof useTradingSummary>);
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

  expect(screen.getByText("Critical divergence")).toBeInTheDocument();
  expect(screen.getByText("Venue lag")).toBeInTheDocument();
  expect(screen.getAllByText("Available")[0]).toBeInTheDocument();
  expect(screen.getByText("rejected")).toBeInTheDocument();
  expect(screen.getByText("filled")).toBeInTheDocument();
  expect(screen.getByText("pending")).toBeInTheDocument();
  expect(screen.getByText("remote_open_orders")).toBeInTheDocument();

  await user.click(screen.getByText("Session and wallet"));
  expect(screen.getAllByText(/fingerprint/).length).toBeGreaterThan(1);
});

test("shows degraded Execution detail panels when live detail queries fail", async () => {
  const user = userEvent.setup();
  mockUseTradingSummary.mockReturnValue({
    isLoading: false,
    data: {
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
        win_rate: 0.66,
        open_trades: 0,
        uptime_hours: 1,
        high_water_mark: 125,
        max_drawdown_pct: 0.1,
        live_session_status: "readonly_ready",
        last_tick_at: null,
        current_window: null,
      },
      real_account_summary: {
        available_cash: 96,
        reserved_cash: 0,
        inventory_mark_value: 2,
        redeemable_value: 1,
        pending_redeem_value: 0,
        total_equity: 99,
        allowance_available: 96,
        latest_snapshot_at_ms: 1100,
        session_id: 1,
        session_status: "readonly_ready",
        session_started_at_ms: 1000,
        wallet_address: "0xwallet",
        proxy_wallet: "0xproxy",
        cash_cap_usd: 100,
        enabled_strategies: ["latency-arb"],
        provider: "polymarket",
        user_stream_status: "ok",
        last_user_stream_connected_at_ms: 1200,
        last_user_stream_event_at_ms: 1250,
        last_account_refresh_at_ms: 1300,
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
    },
  } as ReturnType<typeof useTradingSummary>);
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

  expect(screen.getByText("Detail panels degraded")).toBeInTheDocument();
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

  await user.click(screen.getByText("Session and wallet"));
  expect(
    screen.getByText(/Live session details are currently unavailable: session endpoint failed/),
  ).toBeInTheDocument();
});
