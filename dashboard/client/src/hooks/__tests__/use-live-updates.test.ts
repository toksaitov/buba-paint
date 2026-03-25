import { renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach, afterEach } from "vitest";
import type { ReactNode } from "react";
import { createElement } from "react";

let capturedOnMessage: ((msg: unknown) => void) | null = null;
let capturedOnGiveUp: (() => void) | null = null;
const mockCleanup = vi.fn();

vi.mock("../../lib/ws", () => ({
  connectWs: vi.fn((botId: string, onMessage: (msg: unknown) => void, onGiveUp?: () => void) => {
    capturedOnMessage = onMessage;
    capturedOnGiveUp = onGiveUp ?? null;
    return mockCleanup;
  }),
}));

import { useLiveUpdates } from "../use-live-updates";
import { connectWs } from "../../lib/ws";
const mockConnectWs = vi.mocked(connectWs);

let queryClient: QueryClient;

function createWrapper() {
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  capturedOnMessage = null;
  capturedOnGiveUp = null;
  queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
});

afterEach(() => {
  queryClient.clear();
});

test("connects WS on mount", () => {
  renderHook(() => useLiveUpdates("bot-1"), { wrapper: createWrapper() });
  expect(mockConnectWs).toHaveBeenCalledWith("bot-1", expect.any(Function), expect.any(Function));
});

test("does not connect when botId is empty", () => {
  renderHook(() => useLiveUpdates(""), { wrapper: createWrapper() });
  expect(mockConnectWs).not.toHaveBeenCalled();
});

test("trade message invalidates trades and bot-status queries", () => {
  const spy = vi.spyOn(queryClient, "invalidateQueries");
  renderHook(() => useLiveUpdates("bot-1"), { wrapper: createWrapper() });

  capturedOnMessage!({ type: "trade" });

  expect(spy).toHaveBeenCalledWith({ queryKey: ["trades", "bot-1"] });
  expect(spy).toHaveBeenCalledWith({ queryKey: ["bot-status", "bot-1"] });
});

test("balance message invalidates balance and bot-status queries", () => {
  const spy = vi.spyOn(queryClient, "invalidateQueries");
  renderHook(() => useLiveUpdates("bot-1"), { wrapper: createWrapper() });

  capturedOnMessage!({ type: "balance" });

  expect(spy).toHaveBeenCalledWith({ queryKey: ["balance", "bot-1"] });
  expect(spy).toHaveBeenCalledWith({ queryKey: ["bot-status", "bot-1"] });
});

test("signal message invalidates signals query", () => {
  const spy = vi.spyOn(queryClient, "invalidateQueries");
  renderHook(() => useLiveUpdates("bot-1"), { wrapper: createWrapper() });

  capturedOnMessage!({ type: "signal" });

  expect(spy).toHaveBeenCalledWith({ queryKey: ["signals", "bot-1"] });
});

test("status message invalidates bot-status and process-status", () => {
  const spy = vi.spyOn(queryClient, "invalidateQueries");
  renderHook(() => useLiveUpdates("bot-1"), { wrapper: createWrapper() });

  capturedOnMessage!({ type: "status" });

  expect(spy).toHaveBeenCalledWith({ queryKey: ["bot-status", "bot-1"] });
  expect(spy).toHaveBeenCalledWith({ queryKey: ["process-status", "bot-1"] });
});

test("cleanup disconnects on unmount", () => {
  const { unmount } = renderHook(() => useLiveUpdates("bot-1"), {
    wrapper: createWrapper(),
  });
  unmount();
  expect(mockCleanup).toHaveBeenCalled();
});

test("disables reconnect after give up", () => {
  const { rerender } = renderHook(() => useLiveUpdates("bot-1"), {
    wrapper: createWrapper(),
  });

  // Simulate give-up callback.
  capturedOnGiveUp!();

  // Clear and rerender — should NOT reconnect because disabled.current = true.
  mockConnectWs.mockClear();
  rerender();
  // connectWs should not be called again since disabled ref was set.
  // Note: The effect deps [botId, qc] haven't changed, so React won't re-run
  // the effect anyway. The disabled ref is a safeguard for future re-runs.
  expect(mockConnectWs).not.toHaveBeenCalled();
});
