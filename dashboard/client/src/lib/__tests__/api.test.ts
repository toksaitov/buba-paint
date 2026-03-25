import { describe, expect, test, beforeEach, vi, type Mock } from "vitest";

// We need to mock fetch globally.
const mockFetch = vi.fn() as Mock;
vi.stubGlobal("fetch", mockFetch);

import {
  login,
  getMe,
  getBots,
  getBotStatus,
  getTrades,
  getBalance,
  getSignals,
  getStats,
  getLogs,
  getBotProcessStatus,
  botStart,
  botStop,
  botRestart,
} from "../api";

function jsonResponse(data: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? "OK" : "Error",
    json: () => Promise.resolve(data),
    headers: new Headers(),
  } as unknown as Response;
}

function errorResponse(status: number, error: string): Response {
  return {
    ok: false,
    status,
    statusText: "Error",
    json: () => Promise.resolve({ error }),
    headers: new Headers(),
  } as unknown as Response;
}

beforeEach(() => {
  mockFetch.mockReset();
  localStorage.clear();
});

// -- Internal helpers (headers, get, post) --

describe("auth headers", () => {
  test("includes Authorization when token exists", async () => {
    localStorage.setItem("token", "my-jwt");
    mockFetch.mockResolvedValue(jsonResponse({ bots: [] }));

    await getBots();

    const [, init] = mockFetch.mock.calls[0];
    expect(init.headers["Authorization"]).toBe("Bearer my-jwt");
  });

  test("omits Authorization when no token", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ bots: [] }));

    await getBots();

    const [, init] = mockFetch.mock.calls[0];
    expect(init.headers["Authorization"]).toBeUndefined();
  });
});

describe("get", () => {
  test("sends GET request", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ ok: true }));

    await getBots();

    const [url, init] = mockFetch.mock.calls[0];
    expect(url).toBe("/api/bots");
    expect(init.method).toBeUndefined(); // GET is default
  });

  test("clears token on 401", async () => {
    localStorage.setItem("token", "old-token");
    mockFetch.mockResolvedValue({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      json: () => Promise.resolve({}),
      headers: new Headers(),
    } as unknown as Response);

    await expect(getBots()).rejects.toThrow("Unauthorized");
    expect(localStorage.getItem("token")).toBeNull();
  });

  test("throws on non-ok response", async () => {
    mockFetch.mockResolvedValue(errorResponse(500, "server error"));

    await expect(getBots()).rejects.toThrow("server error");
  });
});

describe("post", () => {
  test("sends POST with JSON body", async () => {
    mockFetch.mockResolvedValue(
      jsonResponse({ token: "jwt", user: { id: "1", username: "admin", role: "admin" } }),
    );

    await login("admin", "pass");

    const [, init] = mockFetch.mock.calls[0];
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual({ username: "admin", password: "pass" });
  });

  test("clears token on 401", async () => {
    localStorage.setItem("token", "old");
    mockFetch.mockResolvedValue({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      json: () => Promise.resolve({}),
      headers: new Headers(),
    } as unknown as Response);

    await expect(botStart("bot-1")).rejects.toThrow("Unauthorized");
    expect(localStorage.getItem("token")).toBeNull();
  });
});

describe("extractError", () => {
  test("parses JSON error field", async () => {
    mockFetch.mockResolvedValue(errorResponse(400, "bad request"));
    await expect(getBots()).rejects.toThrow("bad request");
  });

  test("falls back to status text when no JSON", async () => {
    mockFetch.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Internal Server Error",
      json: () => Promise.reject(new Error("no json")),
      headers: new Headers(),
    } as unknown as Response);

    await expect(getBots()).rejects.toThrow("500 Internal Server Error");
  });
});

// -- Individual API functions --

describe("login", () => {
  test("posts credentials to /api/auth/login", async () => {
    mockFetch.mockResolvedValue(
      jsonResponse({ token: "jwt", user: { id: "1", username: "u", role: "admin" } }),
    );

    await login("user", "pass");

    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe("/api/auth/login");
  });
});

describe("getMe", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ id: "1", username: "u", role: "admin" }));
    await getMe();
    expect(mockFetch.mock.calls[0][0]).toBe("/api/auth/me");
  });
});

describe("getBotStatus", () => {
  test("interpolates botId", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ balance: 200 }));
    await getBotStatus("paint");
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/paint/status");
  });
});

describe("getTrades", () => {
  test("includes page and perPage", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ trades: [], total: 0 }));
    await getTrades("bot-1", 2, 25);
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/bot-1/trades?page=2&per_page=25");
  });
});

describe("getBalance", () => {
  test("includes since", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ entries: [] }));
    await getBalance("bot-1", 1000);
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/bot-1/balance?since=1000");
  });
});

describe("getSignals", () => {
  test("includes limit", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ signals: [] }));
    await getSignals("bot-1", 50);
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/bot-1/signals?limit=50");
  });
});

describe("getStats", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ strategies: [] }));
    await getStats("bot-1");
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/bot-1/stats");
  });
});

describe("getLogs", () => {
  test("includes lines", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ lines: [] }));
    await getLogs("bot-1", 100);
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/bot-1/logs?lines=100");
  });
});

describe("getBotProcessStatus", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: true, pid: 42, uptime_secs: 10 }));
    await getBotProcessStatus("paint");
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/paint/process");
  });
});

describe("bot actions", () => {
  test("botStart posts to correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: true, pid: 1, uptime_secs: 0 }));
    await botStart("paint");
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/paint/start");
  });

  test("botStop posts to correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: false, pid: null, uptime_secs: null }));
    await botStop("paint");
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/paint/stop");
  });

  test("botRestart posts to correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: true, pid: 2, uptime_secs: 0 }));
    await botRestart("paint");
    expect(mockFetch.mock.calls[0][0]).toBe("/api/bots/paint/restart");
  });
});
