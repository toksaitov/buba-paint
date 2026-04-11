import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("react-router-dom", () => ({
  useOutletContext: () => ({ botId: "paint", bot: { id: "paint", name: "Paint" } }),
}));

vi.mock("../../hooks/use-live-status", () => ({
  useLiveStatus: vi.fn(),
  useLiveSessions: vi.fn(),
  useLiveOrders: vi.fn(),
  useLiveFills: vi.fn(),
  useLiveRedemptions: vi.fn(),
  useLiveReconciliation: vi.fn(),
}));

vi.mock("../../components/common/loading", () => ({
  Loading: () => <div data-testid="loading">Loading...</div>,
}));

import { LivePage } from "../live";
import {
  useLiveFills,
  useLiveOrders,
  useLiveReconciliation,
  useLiveRedemptions,
  useLiveSessions,
  useLiveStatus,
} from "../../hooks/use-live-status";

const mockUseLiveStatus = vi.mocked(useLiveStatus);
const mockUseLiveSessions = vi.mocked(useLiveSessions);
const mockUseLiveOrders = vi.mocked(useLiveOrders);
const mockUseLiveFills = vi.mocked(useLiveFills);
const mockUseLiveRedemptions = vi.mocked(useLiveRedemptions);
const mockUseLiveReconciliation = vi.mocked(useLiveReconciliation);

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function W({ children }: { children: ReactNode }) {
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
  mockUseLiveStatus.mockReturnValue({ isLoading: true, data: undefined } as ReturnType<typeof useLiveStatus>);
  render(<LivePage />, { wrapper: createWrapper() });
  expect(screen.getByTestId("loading")).toBeDefined();
});

test("renders empty-state copy when no live session exists", () => {
  mockUseLiveStatus.mockReturnValue({
    isLoading: false,
    data: {
      latest_session: null,
      latest_account_snapshot: null,
      open_orders: 0,
      pending_redemptions: 0,
      critical_reconciliation_events: 0,
    },
  } as ReturnType<typeof useLiveStatus>);

  render(<LivePage />, { wrapper: createWrapper() });

  expect(
    screen.getByText(/No live session has been recorded yet\. That is expected/i),
  ).toBeDefined();
  expect(
    screen.getByText(/The Rust bot still refuses/i),
  ).toBeDefined();
});

test("renders live status summary", () => {
  mockUseLiveStatus.mockReturnValue({
    isLoading: false,
    data: {
      latest_session: {
        id: 1,
        started_at_ms: 1_000,
        ended_at_ms: null,
        status: "readonly_ready",
        execution_mode: "live_readonly",
        wallet_address: "0xwallet",
        proxy_wallet: "0xproxy",
        enabled_strategies_json: "[\"latency-arb\"]",
        config_fingerprint: "fp",
        cash_cap_usd: 100,
        details_json: "{\"provider\":\"stub\"}",
      },
      latest_account_snapshot: {
        id: 1,
        session_id: 1,
        timestamp_ms: 1_100,
        cash_available: 96,
        cash_reserved_for_orders: 0,
        inventory_mark_value: 2,
        redeemable_value: 1,
        pending_redeem_value: 0,
        total_equity: 99,
        allowance_available: 96,
        details_json: "{\"provider\":\"stub\"}",
      },
      open_orders: 1,
      pending_redemptions: 1,
      critical_reconciliation_events: 1,
    },
  } as ReturnType<typeof useLiveStatus>);

  render(<LivePage />, { wrapper: createWrapper() });

  expect(screen.getByText("Live Readiness")).toBeDefined();
  expect(screen.getAllByText("live_readonly").length).toBeGreaterThan(0);
  expect(screen.getByText(/Provider status: stub/i)).toBeDefined();
  expect(screen.getByText(/Fingerprint fp/i)).toBeDefined();
  expect(screen.getAllByText("$96.00").length).toBeGreaterThan(0);
});
