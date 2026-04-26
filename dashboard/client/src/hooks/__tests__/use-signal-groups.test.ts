import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import { useSignalGroups } from "../use-signal-groups";

vi.mock("../../lib/api", () => ({
  getSignalGroups: vi.fn(),
}));

import { getSignalGroups } from "../../lib/api";

const mockGetSignalGroups = vi.mocked(getSignalGroups);

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

test("returns grouped signal bursts", async () => {
  mockGetSignalGroups.mockResolvedValue({
    groups: [
      {
        id: "mkt-1:latency-arb:UP:1000",
        first_timestamp: 1000,
        last_timestamp: 1200,
        count: 2,
        strategy: "latency-arb",
        direction: "UP",
        market_id: "mkt-1",
        binance_price: 70000,
        chainlink_price: 70010,
        up_ask: 0.52,
        down_ask: 0.48,
        momentum: null,
      },
    ],
  });

  const { result } = renderHook(() => useSignalGroups("bot-1"), {
    wrapper: createWrapper(),
  });

  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.groups[0]?.count).toBe(2);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useSignalGroups(""), {
    wrapper: createWrapper(),
  });

  expect(result.current.isFetching).toBe(false);
  expect(mockGetSignalGroups).not.toHaveBeenCalled();
});
