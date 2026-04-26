import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, vi } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";
import { useEquitySeries } from "../use-equity-series";

vi.mock("../../lib/api", () => ({
  getEquitySeries: vi.fn(),
}));

import { getEquitySeries } from "../../lib/api";

const mockGetEquitySeries = vi.mocked(getEquitySeries);

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

test("returns chart-safe equity series data", async () => {
  mockGetEquitySeries.mockResolvedValue({
    baseline: { id: 1, timestamp: 0, event: "baseline", balance: 200 },
    points: [{ id: 2, timestamp: 1000, event: "settlement", balance: 210 }],
  });

  const { result } = renderHook(() => useEquitySeries("bot-1"), {
    wrapper: createWrapper(),
  });

  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.baseline?.timestamp).toBe(0);
  expect(result.current.data?.points[0]?.timestamp).toBe(1000);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useEquitySeries(""), {
    wrapper: createWrapper(),
  });

  expect(result.current.isFetching).toBe(false);
  expect(mockGetEquitySeries).not.toHaveBeenCalled();
});
