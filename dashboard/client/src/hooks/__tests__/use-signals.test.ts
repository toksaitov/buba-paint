import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useSignals } from "../use-signals";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getSignals: vi.fn(),
}));

import { getSignals } from "../../lib/api";
const mockGetSignals = vi.mocked(getSignals);

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
  mockGetSignals.mockResolvedValue({ signals: [{ id: 1, strategy: "arb" }] });

  const { result } = renderHook(() => useSignals("bot-1", 50), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data.signals).toHaveLength(1);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useSignals(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetSignals).not.toHaveBeenCalled();
});

test("handles error", async () => {
  mockGetSignals.mockRejectedValue(new Error("fail"));

  const { result } = renderHook(() => useSignals("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("fail");
});
