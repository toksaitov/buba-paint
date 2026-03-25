import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useTrades } from "../use-trades";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getTrades: vi.fn(),
}));

import { getTrades } from "../../lib/api";
const mockGetTrades = vi.mocked(getTrades);

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

test("returns data on success", async () => {
  mockGetTrades.mockResolvedValue({ trades: [{ id: 1 }], total: 1 });

  const { result } = renderHook(() => useTrades("bot-1", 1, 50), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data.trades).toHaveLength(1);
  expect(result.current.data.total).toBe(1);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useTrades(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetTrades).not.toHaveBeenCalled();
});

test("handles error", async () => {
  mockGetTrades.mockRejectedValue(new Error("fail"));

  const { result } = renderHook(() => useTrades("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("fail");
});
