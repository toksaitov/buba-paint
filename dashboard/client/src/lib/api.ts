import type {
  BalanceResponse,
  Bot,
  BotProcessStatus,
  BotStatus,
  EquitySeriesResponse,
  LiveFillsResponse,
  LiveOrdersResponse,
  LiveReconciliationResponse,
  LiveRedemptionsResponse,
  LiveSessionsResponse,
  LogsResponse,
  SignalGroupsResponse,
  SignalsResponse,
  StatsResponse,
  TradingSummary,
  TradesResponse,
  User,
} from "./types";
import { useAuthStore } from "../stores/auth-store";

const BASE = "";

function handleUnauthorized() {
  useAuthStore.getState().logout();
  window.location.href = "/login";
}

function headers(): HeadersInit {
  const h: HeadersInit = { "Content-Type": "application/json" };
  const token = localStorage.getItem("token");
  if (token) h["Authorization"] = `Bearer ${token}`;
  return h;
}

async function extractError(res: Response): Promise<string> {
  try {
    const json = await res.json();
    if (json && typeof json.error === "string") return json.error;
  } catch {}
  return `${res.status} ${res.statusText}`;
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { headers: headers() });
  if (res.status === 401) {
    handleUnauthorized();
    throw new Error("Unauthorized");
  }
  if (!res.ok) throw new Error(await extractError(res));
  return res.json() as Promise<T>;
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: headers(),
    body: body ? JSON.stringify(body) : undefined,
  });
  if (res.status === 401) {
    handleUnauthorized();
    throw new Error("Unauthorized");
  }
  if (!res.ok) throw new Error(await extractError(res));
  return res.json() as Promise<T>;
}

export async function login(
  username: string,
  password: string,
): Promise<{ token: string; user: User }> {
  const res = await fetch(`${BASE}/api/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) throw new Error(await extractError(res));
  return res.json() as Promise<{ token: string; user: User }>;
}

export async function getMe(): Promise<User> {
  return get("/api/auth/me");
}

export async function getBots(): Promise<{ bots: Bot[] }> {
  return get("/api/bots");
}

export async function getBotStatus(botId: string): Promise<BotStatus> {
  return get(`/api/bots/${botId}/status`);
}

export async function getTrades(
  botId: string,
  page = 1,
  perPage = 50,
): Promise<TradesResponse> {
  return get(`/api/bots/${botId}/trades?page=${page}&per_page=${perPage}`);
}

export async function getBalance(
  botId: string,
  since = 0,
): Promise<BalanceResponse> {
  return get(`/api/bots/${botId}/balance?since=${since}`);
}

export async function getEquitySeries(
  botId: string,
  since = 0,
): Promise<EquitySeriesResponse> {
  return get(`/api/bots/${botId}/equity/series?since=${since}`);
}

export async function getSignals(
  botId: string,
  limit = 100,
): Promise<SignalsResponse> {
  return get(`/api/bots/${botId}/signals?limit=${limit}`);
}

export async function getSignalGroups(
  botId: string,
  limit = 50,
  quietGapMs = 5000,
): Promise<SignalGroupsResponse> {
  return get(
    `/api/bots/${botId}/signals/groups?limit=${limit}&quiet_gap_ms=${quietGapMs}`,
  );
}

export async function getStats(botId: string): Promise<StatsResponse> {
  return get(`/api/bots/${botId}/stats`);
}

export async function getTradingSummary(botId: string): Promise<TradingSummary> {
  return get(`/api/bots/${botId}/trading/summary`);
}

export async function getLogs(
  botId: string,
  lines = 200,
): Promise<LogsResponse> {
  return get(`/api/bots/${botId}/logs?lines=${lines}`);
}

export async function getBotProcessStatus(
  botId: string,
): Promise<BotProcessStatus> {
  return get(`/api/bots/${botId}/process`);
}

export async function getLiveSessions(
  botId: string,
  limit = 20,
): Promise<LiveSessionsResponse> {
  return get(`/api/bots/${botId}/live/sessions?limit=${limit}`);
}

export async function getLiveOrders(
  botId: string,
  limit = 50,
): Promise<LiveOrdersResponse> {
  return get(`/api/bots/${botId}/live/orders?limit=${limit}`);
}

export async function getLiveFills(
  botId: string,
  limit = 50,
): Promise<LiveFillsResponse> {
  return get(`/api/bots/${botId}/live/fills?limit=${limit}`);
}

export async function getLiveRedemptions(
  botId: string,
  limit = 50,
): Promise<LiveRedemptionsResponse> {
  return get(`/api/bots/${botId}/live/redemptions?limit=${limit}`);
}

export async function getLiveReconciliation(
  botId: string,
  limit = 50,
): Promise<LiveReconciliationResponse> {
  return get(`/api/bots/${botId}/live/reconciliation?limit=${limit}`);
}

export async function botStart(botId: string): Promise<BotProcessStatus> {
  return post(`/api/bots/${botId}/start`);
}

export async function botStop(botId: string): Promise<BotProcessStatus> {
  return post(`/api/bots/${botId}/stop`);
}

export async function botRestart(botId: string): Promise<BotProcessStatus> {
  return post(`/api/bots/${botId}/restart`);
}
