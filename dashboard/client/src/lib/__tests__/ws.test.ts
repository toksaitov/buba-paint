import { describe, expect, test, beforeEach, vi, afterEach } from "vitest";

// Minimal WebSocket mock.
class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  readyState = 0;
  closeCalled = false;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    this.closeCalled = true;
  }

  /** Simulate the server opening the connection. */
  simulateOpen() {
    this.readyState = 1;
    this.onopen?.();
  }

  /** Simulate the server sending a message. */
  simulateMessage(data: string) {
    this.onmessage?.({ data });
  }

  /** Simulate the connection closing. */
  simulateClose() {
    this.readyState = 3;
    this.onclose?.();
  }

  /** Simulate an error. */
  simulateError() {
    this.onerror?.();
  }
}

vi.stubGlobal("WebSocket", MockWebSocket);

import { connectWs } from "../ws";

beforeEach(() => {
  MockWebSocket.instances = [];
  localStorage.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("connectWs", () => {
  test("builds URL with token from localStorage", () => {
    localStorage.setItem("token", "my-jwt");
    connectWs("bot-1", vi.fn());

    const ws = MockWebSocket.instances[0];
    expect(ws.url).toContain("/ws/bots/bot-1");
    expect(ws.url).toContain("?token=my-jwt");
  });

  test("builds URL without token when none in localStorage", () => {
    connectWs("bot-1", vi.fn());

    const ws = MockWebSocket.instances[0];
    expect(ws.url).toContain("/ws/bots/bot-1");
    expect(ws.url).not.toContain("?token");
  });

  test("onMessage receives parsed JSON", () => {
    const onMessage = vi.fn();
    connectWs("bot-1", onMessage);

    const ws = MockWebSocket.instances[0];
    ws.simulateOpen();
    ws.simulateMessage(JSON.stringify({ type: "trade", data: { id: 1 } }));

    expect(onMessage).toHaveBeenCalledWith({ type: "trade", data: { id: 1 } });
  });

  test("onMessage ignores unparseable data", () => {
    const onMessage = vi.fn();
    connectWs("bot-1", onMessage);

    const ws = MockWebSocket.instances[0];
    ws.simulateOpen();
    ws.simulateMessage("not json{{{");

    expect(onMessage).not.toHaveBeenCalled();
  });

  test("cleanup closes WebSocket", () => {
    const cleanup = connectWs("bot-1", vi.fn());

    const ws = MockWebSocket.instances[0];
    ws.simulateOpen();
    cleanup();

    expect(ws.closeCalled).toBe(true);
  });

  test("reconnects on close", () => {
    connectWs("bot-1", vi.fn());

    const ws1 = MockWebSocket.instances[0];
    ws1.simulateOpen();
    ws1.simulateClose();

    // First retry delay: 3000 * 1 = 3000ms
    expect(MockWebSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(3000);
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  test("calls onGiveUp after max retries", () => {
    const onGiveUp = vi.fn();
    connectWs("bot-1", vi.fn(), onGiveUp);

    // Fail 4 times (MAX_RETRIES = 3, so > 3 failures triggers give-up).
    for (let i = 0; i < 4; i++) {
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.simulateClose();
      // Advance past the reconnect delay.
      vi.advanceTimersByTime(15000);
    }

    expect(onGiveUp).toHaveBeenCalledTimes(1);
  });

  test("resets failure count on successful open", () => {
    const onGiveUp = vi.fn();
    connectWs("bot-1", vi.fn(), onGiveUp);

    // Fail twice.
    const ws1 = MockWebSocket.instances[0];
    ws1.simulateClose();
    vi.advanceTimersByTime(3000);

    const ws2 = MockWebSocket.instances[1];
    ws2.simulateClose();
    vi.advanceTimersByTime(6000);

    // Now succeed, which resets failures to 0.
    const ws3 = MockWebSocket.instances[2];
    ws3.simulateOpen();

    // Fail 3 more times — should NOT give up because counter was reset.
    ws3.simulateClose();
    vi.advanceTimersByTime(3000);

    const ws4 = MockWebSocket.instances[3];
    ws4.simulateClose();
    vi.advanceTimersByTime(6000);

    const ws5 = MockWebSocket.instances[4];
    ws5.simulateClose();
    vi.advanceTimersByTime(9000);

    // 3 failures after reset — still within MAX_RETRIES, should reconnect.
    expect(MockWebSocket.instances).toHaveLength(6);
    expect(onGiveUp).not.toHaveBeenCalled();
  });
});
