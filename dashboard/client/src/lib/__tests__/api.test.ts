import { describe, expect, test, beforeEach, vi, type Mock } from "vitest";

const mockFetch = vi.fn() as Mock;
vi.stubGlobal("fetch", mockFetch);

import {
  login,
  getMe,
  getBots,
  getBotStatus,
  getTrades,
  getBalance,
  getEquitySeries,
  getSignals,
  getSignalGroups,
  getStats,
  getTradingSummary,
  getLogs,
  getBotProcessStatus,
  botStart,
  botStop,
  botRestart,
} from "../api";
import { useAuthStore } from "../../stores/auth-store";

function jsonResponse(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    statusText: status === 200 ? "OK" : "Error",
    headers: { "Content-Type": "application/json" },
  });
}

function errorResponse(status: number, error: string): Response {
  return new Response(JSON.stringify({ error }), {
    status,
    statusText: "Error",
    headers: { "Content-Type": "application/json" },
  });
}

beforeEach(() => {
  mockFetch.mockReset();
  localStorage.clear();
});

function pathFromCall(): string {
  const [input] = mockFetch.mock.calls[0] as [Request | string, RequestInit | undefined];
  if (typeof input === "string") return input;
  if (/^https?:/i.test(input.url)) {
    const url = new URL(input.url);
    return `${url.pathname}${url.search}`;
  }
  return input.url;
}

function headersFromCall(): Headers {
  const [input, init] = mockFetch.mock.calls[0] as [Request | string, RequestInit | undefined];
  if (typeof input !== "string") return input.headers;
  return new Headers(init?.headers);
}

function methodFromCall(): string {
  const [input, init] = mockFetch.mock.calls[0] as [Request | string, RequestInit | undefined];
  if (typeof input !== "string") return input.method;
  return init?.method ?? "GET";
}

async function bodyFromCall(): Promise<string> {
  const [input, init] = mockFetch.mock.calls[0] as [Request | string, RequestInit | undefined];
  if (typeof input !== "string") return input.clone().text();
  return typeof init?.body === "string" ? init.body : "";
}

describe("auth headers", () => {
  test("includes Authorization when token exists", async () => {
    localStorage.setItem("token", "my-jwt");
    mockFetch.mockResolvedValue(jsonResponse({ bots: [] }));

    await getBots();

    expect(headersFromCall().get("Authorization")).toBe("Bearer my-jwt");
  });

  test("omits Authorization when no token", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ bots: [] }));

    await getBots();

    expect(headersFromCall().get("Authorization")).toBeNull();
  });
});

describe("get", () => {
  test("sends GET request", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ ok: true }));

    await getBots();

    expect(pathFromCall()).toBe("/api/bots");
    expect(methodFromCall()).toBe("GET");
  });

  test("clears token on 401", async () => {
    localStorage.setItem("token", "old-token");
    useAuthStore.getState().setAuth("old-token", {
      id: "1",
      username: "admin",
      role: "admin",
    });
    mockFetch.mockResolvedValue(
      new Response(JSON.stringify({}), {
        status: 401,
        statusText: "Unauthorized",
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(getBots()).rejects.toThrow("Unauthorized");
    expect(localStorage.getItem("token")).toBeNull();
    expect(useAuthStore.getState().token).toBeNull();
    expect(useAuthStore.getState().user).toBeNull();
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

    expect(methodFromCall()).toBe("POST");
    expect(JSON.parse(await bodyFromCall())).toEqual({
      username: "admin",
      password: "pass",
    });
  });

  test("clears token on 401", async () => {
    localStorage.setItem("token", "old");
    useAuthStore.getState().setAuth("old", {
      id: "1",
      username: "admin",
      role: "admin",
    });
    mockFetch.mockResolvedValue(
      new Response(JSON.stringify({}), {
        status: 401,
        statusText: "Unauthorized",
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(botStart("bot-1")).rejects.toThrow("Unauthorized");
    expect(localStorage.getItem("token")).toBeNull();
    expect(useAuthStore.getState().token).toBeNull();
    expect(useAuthStore.getState().user).toBeNull();
  });
});

describe("extractError", () => {
  test("parses JSON error field", async () => {
    mockFetch.mockResolvedValue(errorResponse(400, "bad request"));
    await expect(getBots()).rejects.toThrow("bad request");
  });

  test("falls back to status text when no JSON", async () => {
    const response = new Response("", {
      status: 500,
      statusText: "Internal Server Error",
    });
    Object.defineProperty(response, "json", {
      value: () => Promise.reject(new Error("no json")),
    });
    mockFetch.mockResolvedValue(response);

    await expect(getBots()).rejects.toThrow("500 Internal Server Error");
  });
});

describe("login", () => {
  test("posts credentials to /api/auth/login", async () => {
    mockFetch.mockResolvedValue(
      jsonResponse({ token: "jwt", user: { id: "1", username: "u", role: "admin" } }),
    );

    await login("user", "pass");

    expect(pathFromCall()).toBe("/api/auth/login");
  });
});

describe("getMe", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ id: "1", username: "u", role: "admin" }));
    await getMe();
    expect(pathFromCall()).toBe("/api/auth/me");
  });
});

describe("getBotStatus", () => {
  test("interpolates botId", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ balance: 200 }));
    await getBotStatus("paint");
    expect(pathFromCall()).toBe("/api/bots/paint/status");
  });
});

describe("getTrades", () => {
  test("includes page and perPage", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ trades: [], total: 0 }));
    await getTrades("bot-1", 2, 25);
    expect(pathFromCall()).toBe("/api/bots/bot-1/trades?page=2&per_page=25");
  });
});

describe("getBalance", () => {
  test("includes since", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ entries: [] }));
    await getBalance("bot-1", 1000);
    expect(pathFromCall()).toBe("/api/bots/bot-1/balance?since=1000");
  });
});

describe("getEquitySeries", () => {
  test("includes since", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ baseline: null, points: [] }));
    await getEquitySeries("bot-1", 1000);
    expect(pathFromCall()).toBe("/api/bots/bot-1/equity/series?since=1000");
  });
});

describe("getSignals", () => {
  test("includes limit", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ signals: [] }));
    await getSignals("bot-1", 50);
    expect(pathFromCall()).toBe("/api/bots/bot-1/signals?limit=50");
  });
});

describe("getSignalGroups", () => {
  test("includes limit and quiet gap", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ groups: [] }));
    await getSignalGroups("bot-1", 25, 2000);
    expect(pathFromCall()).toBe("/api/bots/bot-1/signals/groups?limit=25&quiet_gap_ms=2000");
  });
});

describe("getStats", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ strategies: [] }));
    await getStats("bot-1");
    expect(pathFromCall()).toBe("/api/bots/bot-1/stats");
  });
});

describe("getTradingSummary", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ runtime_mode: "paper" }));
    await getTradingSummary("bot-1");
    expect(pathFromCall()).toBe("/api/bots/bot-1/trading/summary");
  });
});

describe("getLogs", () => {
  test("includes lines", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ lines: [] }));
    await getLogs("bot-1", 100);
    expect(pathFromCall()).toBe("/api/bots/bot-1/logs?lines=100");
  });
});

describe("getBotProcessStatus", () => {
  test("calls correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: true, pid: 42, uptime_secs: 10 }));
    await getBotProcessStatus("paint");
    expect(pathFromCall()).toBe("/api/bots/paint/process");
  });
});

describe("bot actions", () => {
  test("botStart posts to correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: true, pid: 1, uptime_secs: 0 }));
    await botStart("paint");
    expect(pathFromCall()).toBe("/api/bots/paint/start");
  });

  test("botStop posts to correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: false, pid: null, uptime_secs: null }));
    await botStop("paint");
    expect(pathFromCall()).toBe("/api/bots/paint/stop");
  });

  test("botRestart posts to correct URL", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ active: true, pid: 2, uptime_secs: 0 }));
    await botRestart("paint");
    expect(pathFromCall()).toBe("/api/bots/paint/restart");
  });
});
