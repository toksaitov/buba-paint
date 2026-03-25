import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useProcessStatus } from "../use-process-status";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getBotProcessStatus: vi.fn(),
}));

import { getBotProcessStatus } from "../../lib/api";
const mockGetStatus = vi.mocked(getBotProcessStatus);

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

test("returns process status when active", async () => {
  mockGetStatus.mockResolvedValue({
    active: true,
    pid: 1234,
    uptime_secs: 300,
  });

  const { result } = renderHook(() => useProcessStatus("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.active).toBe(true);
  expect(result.current.data?.pid).toBe(1234);
  expect(result.current.data?.uptime_secs).toBe(300);
});

test("returns inactive when bot is stopped", async () => {
  mockGetStatus.mockResolvedValue({
    active: false,
    pid: null,
    uptime_secs: null,
  });

  const { result } = renderHook(() => useProcessStatus("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.active).toBe(false);
  expect(result.current.data?.pid).toBeNull();
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useProcessStatus(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetStatus).not.toHaveBeenCalled();
});
