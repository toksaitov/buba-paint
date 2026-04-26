import { EventEmitter } from "node:events";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WsUserStreamMonitor } from "../provider.js";

class FakeSocket extends EventEmitter {
  readyState = 0;
  sent: string[] = [];
  closeCalls = 0;
  terminateCalls = 0;

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closeCalls += 1;
    this.readyState = 3;
    this.emit("close", 1000, Buffer.from("closed"));
  }

  terminate(): void {
    this.terminateCalls += 1;
    this.readyState = 3;
    this.emit("close", 1006, Buffer.from("terminated"));
  }

  open(): void {
    this.readyState = 1;
    this.emit("open");
  }

  message(payload: unknown): void {
    this.emit("message", Buffer.from(JSON.stringify(payload)));
  }

  fail(error: string): void {
    this.emit("error", new Error(error));
  }

  disconnect(code = 1006, reason = "closed"): void {
    this.readyState = 3;
    this.emit("close", code, Buffer.from(reason));
  }
}

describe("WsUserStreamMonitor", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  function createMonitor() {
    vi.useFakeTimers();
    const sockets: FakeSocket[] = [];
    const monitor = new WsUserStreamMonitor({
      now: () => 1_700_000_000_000,
      createSocket: () => {
        const socket = new FakeSocket();
        sockets.push(socket);
        return socket;
      },
      setTimeoutFn: setTimeout,
      clearTimeoutFn: clearTimeout,
      random: () => 0.5,
      log: () => undefined,
      connectTimeoutMs: 50,
      stableGraceMs: 10,
      reconnectBaseMs: 20,
      reconnectMaxMs: 100,
    });
    return { monitor, sockets };
  }

  async function connectMonitor(
    monitor: WsUserStreamMonitor,
    sockets: FakeSocket[],
    markets: string[] = ["mkt-a"],
  ): Promise<void> {
    const promise = monitor.ensureConnected(
      { key: "k", secret: "s", passphrase: "p" },
      markets,
    );
    sockets[0].open();
    sockets[0].message({ event_type: "subscribed" });
    await promise;
  }

  it("does not crash when closed during CONNECTING", async () => {
    const { monitor, sockets } = createMonitor();

    const connectPromise = monitor
      .ensureConnected(
        { key: "k", secret: "s", passphrase: "p" },
        ["mkt-a"],
      )
      .catch(() => undefined);

    expect(sockets).toHaveLength(1);
    monitor.close();
    await vi.runAllTimersAsync();
    await connectPromise;

    expect(sockets[0].terminateCalls).toBe(1);
    expect(monitor.snapshot().lifecycle).toBe("closed");
  });

  it("handles an error before open without crashing and schedules reconnect", async () => {
    const { monitor, sockets } = createMonitor();

    const connectPromise = monitor
      .ensureConnected(
        { key: "k", secret: "s", passphrase: "p" },
        ["mkt-a"],
      )
      .catch((error) => error as Error);

    sockets[0].fail("boom");
    const error = (await connectPromise) as Error;

    expect(error.message).toContain("user_stream_connect: boom");
    expect(monitor.snapshot().status).toBe("failed");
    await vi.advanceTimersByTimeAsync(25);
    expect(sockets).toHaveLength(2);
  });

  it("marks a connect timeout as failed and schedules reconnect", async () => {
    const { monitor, sockets } = createMonitor();

    const connectPromise = monitor
      .ensureConnected(
        { key: "k", secret: "s", passphrase: "p" },
        ["mkt-a"],
      )
      .catch((error) => error as Error);

    await vi.advanceTimersByTimeAsync(55);
    const error = (await connectPromise) as Error;

    expect(error.message).toContain("timed out connecting");
    expect(monitor.snapshot().consecutiveFailures).toBe(1);
    await vi.advanceTimersByTimeAsync(25);
    expect(sockets).toHaveLength(2);
  });

  it("succeeds when a message arrives before the stable grace timer elapses", async () => {
    const { monitor, sockets } = createMonitor();

    const connectPromise = monitor.ensureConnected(
      { key: "k", secret: "s", passphrase: "p" },
      ["mkt-a"],
    );

    sockets[0].open();
    sockets[0].message({ event_type: "subscribed" });
    await connectPromise;

    expect(monitor.snapshot().status).toBe("ok");
    expect(monitor.snapshot().subscribedMarkets).toEqual(["mkt-a"]);
  });

  it("fails cleanly on an auth error frame", async () => {
    const { monitor, sockets } = createMonitor();

    const connectPromise = monitor
      .ensureConnected(
        { key: "k", secret: "s", passphrase: "p" },
        ["mkt-a"],
      )
      .catch((error) => error as Error);

    sockets[0].open();
    sockets[0].message({ type: "error", error: "auth rejected" });
    const error = (await connectPromise) as Error;

    expect(error.message).toContain("auth rejected");
    expect(monitor.snapshot().status).toBe("failed");
  });

  it("reconnects after a post-connect close", async () => {
    const { monitor, sockets } = createMonitor();

    await connectMonitor(monitor, sockets);

    sockets[0].disconnect(1006, "bye");
    await vi.advanceTimersByTimeAsync(25);

    expect(sockets).toHaveLength(2);
    expect(monitor.snapshot().lifecycle).toBe("reconnecting");
  });

  it("prevents reconnect after explicit shutdown", async () => {
    const { monitor, sockets } = createMonitor();

    await connectMonitor(monitor, sockets);

    sockets[0].disconnect(1006, "bye");
    monitor.close();
    await vi.advanceTimersByTimeAsync(100);

    expect(sockets).toHaveLength(1);
    expect(monitor.snapshot().lifecycle).toBe("closed");
  });

  it("does not reconnect when the desired markets are unchanged", async () => {
    const { monitor, sockets } = createMonitor();

    await connectMonitor(monitor, sockets);
    await monitor.ensureConnected(
      { key: "k", secret: "s", passphrase: "p" },
      ["mkt-a"],
    );

    expect(sockets).toHaveLength(1);
  });

  it("reconnects once when the desired markets change", async () => {
    const { monitor, sockets } = createMonitor();

    await connectMonitor(monitor, sockets);

    const nextPromise = monitor.ensureConnected(
      { key: "k", secret: "s", passphrase: "p" },
      ["mkt-b"],
    );

    await Promise.resolve();
    expect(sockets[0].closeCalls + sockets[0].terminateCalls).toBeGreaterThan(0);
    sockets[1].open();
    sockets[1].message({ event_type: "subscribed" });
    await nextPromise;

    expect(sockets).toHaveLength(2);
    expect(monitor.snapshot().subscribedMarkets).toEqual(["mkt-b"]);
  });

  it("drives one final connection for the latest desired market set", async () => {
    const { monitor, sockets } = createMonitor();

    const firstPromise = monitor.ensureConnected(
      { key: "k", secret: "s", passphrase: "p" },
      ["mkt-a"],
    );
    const secondPromise = monitor.ensureConnected(
      { key: "k", secret: "s", passphrase: "p" },
      ["mkt-b"],
    );

    sockets[0].open();
    sockets[0].message({ event_type: "subscribed" });
    await Promise.resolve();
    await Promise.resolve();
    expect(sockets).toHaveLength(2);
    sockets[1].open();
    sockets[1].message({ event_type: "subscribed" });
    await Promise.all([firstPromise, secondPromise]);

    expect(sockets).toHaveLength(2);
    expect(monitor.snapshot().subscribedMarkets).toEqual(["mkt-b"]);
  });
});
