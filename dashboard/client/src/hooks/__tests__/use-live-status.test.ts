import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import {
  useLiveFills,
  useLiveOrders,
  useLiveReconciliation,
  useLiveRedemptions,
  useLiveSessions,
} from "../use-live-status";

vi.mock("../../lib/api", () => ({
  getLiveSessions: vi.fn(),
  getLiveOrders: vi.fn(),
  getLiveFills: vi.fn(),
  getLiveRedemptions: vi.fn(),
  getLiveReconciliation: vi.fn(),
}));

import {
  getLiveFills,
  getLiveOrders,
  getLiveReconciliation,
  getLiveRedemptions,
  getLiveSessions,
} from "../../lib/api";

const mockGetLiveSessions = vi.mocked(getLiveSessions);
const mockGetLiveOrders = vi.mocked(getLiveOrders);
const mockGetLiveFills = vi.mocked(getLiveFills);
const mockGetLiveRedemptions = vi.mocked(getLiveRedemptions);
const mockGetLiveReconciliation = vi.mocked(getLiveReconciliation);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

afterEach(() => {
  vi.clearAllMocks();
});

test("live detail hooks use limits and stay idle without a bot id", async () => {
  mockGetLiveSessions.mockResolvedValue({ sessions: [] });
  mockGetLiveOrders.mockResolvedValue({ orders: [] });
  mockGetLiveFills.mockResolvedValue({ fills: [] });
  mockGetLiveRedemptions.mockResolvedValue({ redemptions: [] });
  mockGetLiveReconciliation.mockResolvedValue({ events: [] });

  const wrapper = createWrapper();
  const sessions = renderHook(() => useLiveSessions("bot-1", 6), { wrapper });
  const orders = renderHook(() => useLiveOrders("bot-1", 7), { wrapper });
  const fills = renderHook(() => useLiveFills("bot-1", 8), { wrapper });
  const redemptions = renderHook(() => useLiveRedemptions("bot-1", 9), { wrapper });
  const reconciliation = renderHook(() => useLiveReconciliation("bot-1", 10), { wrapper });
  const idle = renderHook(() => useLiveSessions("", 6), { wrapper });

  await waitFor(() => expect(sessions.result.current.data).toBeDefined());
  await waitFor(() => expect(orders.result.current.data).toBeDefined());
  await waitFor(() => expect(fills.result.current.data).toBeDefined());
  await waitFor(() => expect(redemptions.result.current.data).toBeDefined());
  await waitFor(() => expect(reconciliation.result.current.data).toBeDefined());

  expect(mockGetLiveSessions).toHaveBeenCalledWith("bot-1", 6);
  expect(mockGetLiveOrders).toHaveBeenCalledWith("bot-1", 7);
  expect(mockGetLiveFills).toHaveBeenCalledWith("bot-1", 8);
  expect(mockGetLiveRedemptions).toHaveBeenCalledWith("bot-1", 9);
  expect(mockGetLiveReconciliation).toHaveBeenCalledWith("bot-1", 10);
  expect(idle.result.current.isFetching).toBe(false);
});
