import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useLogs } from "../use-logs";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getLogs: vi.fn(),
}));

import { getLogs } from "../../lib/api";
const mockGetLogs = vi.mocked(getLogs);

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
  mockGetLogs.mockResolvedValue({ lines: ["line1", "line2"] });

  const { result } = renderHook(() => useLogs("bot-1", 100), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data.lines).toHaveLength(2);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useLogs(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetLogs).not.toHaveBeenCalled();
});

test("handles error", async () => {
  mockGetLogs.mockRejectedValue(new Error("fail"));

  const { result } = renderHook(() => useLogs("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("fail");
});
