import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useBalance } from "../use-balance";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getBalance: vi.fn(),
}));

import { getBalance } from "../../lib/api";
const mockGetBalance = vi.mocked(getBalance);

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
  mockGetBalance.mockResolvedValue({ entries: [{ balance: 200 }] });

  const { result } = renderHook(() => useBalance("bot-1", 1000), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data.entries).toHaveLength(1);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useBalance(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetBalance).not.toHaveBeenCalled();
});

test("handles error", async () => {
  mockGetBalance.mockRejectedValue(new Error("fail"));

  const { result } = renderHook(() => useBalance("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("fail");
});
