import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { useBotStatus } from "../use-bot-status";
import type { ReactNode } from "react";
import { createElement } from "react";

vi.mock("../../lib/api", () => ({
  getBotStatus: vi.fn(),
}));

import { getBotStatus } from "../../lib/api";
const mockGetBotStatus = vi.mocked(getBotStatus);

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
  mockGetBotStatus.mockResolvedValue({ balance: 500, open_trades: 2 });

  const { result } = renderHook(() => useBotStatus("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data.balance).toBe(500);
  expect(result.current.data.open_trades).toBe(2);
});

test("does not fetch when botId is empty", () => {
  const { result } = renderHook(() => useBotStatus(""), {
    wrapper: createWrapper(),
  });
  expect(result.current.isFetching).toBe(false);
  expect(mockGetBotStatus).not.toHaveBeenCalled();
});

test("handles error", async () => {
  mockGetBotStatus.mockRejectedValue(new Error("fail"));

  const { result } = renderHook(() => useBotStatus("bot-1"), {
    wrapper: createWrapper(),
  });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(result.current.error?.message).toBe("fail");
});
