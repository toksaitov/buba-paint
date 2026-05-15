import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useRuntimeConfig } from "../use-runtime-config";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getRuntimeConfig: vi.fn(),
}));

import { getRuntimeConfig } from "../../lib/api";
const mockGetRuntimeConfig = vi.mocked(getRuntimeConfig);

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
  mockGetRuntimeConfig.mockResolvedValue({
    snapshot: null,
    snapshot_recorded_at_ms: null,
    uptime_secs: null,
  });

  const { result } = renderHook(() => useRuntimeConfig("paint"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.snapshot).toBeNull();
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useRuntimeConfig(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetRuntimeConfig).not.toHaveBeenCalled();
});

test("propagates fetch errors", async () => {
  mockGetRuntimeConfig.mockRejectedValue(new Error("network down"));

  const { result } = renderHook(() => useRuntimeConfig("paint"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("network down");
});
