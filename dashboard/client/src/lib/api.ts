import type {
  BalanceResponse,
  Bot,
  BotProcessStatus,
  BotStatus,
  LogsResponse,
  SignalsResponse,
  StatsResponse,
  TradesResponse,
  User,
} from "./types";

const BASE = "";

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
  } catch {
    /* no json body */
  }
  return `${res.status} ${res.statusText}`;
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { headers: headers() });
  if (res.status === 401) {
    localStorage.removeItem("token");
    window.location.href = "/login";
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
    localStorage.removeItem("token");
    window.location.href = "/login";
    throw new Error("Unauthorized");
  }
  if (!res.ok) throw new Error(await extractError(res));
  return res.json() as Promise<T>;
}

export async function login(
  username: string,
  password: string,
): Promise<{ token: string; user: User }> {
  return post("/api/auth/login", { username, password });
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

export async function getSignals(
  botId: string,
  limit = 100,
): Promise<SignalsResponse> {
  return get(`/api/bots/${botId}/signals?limit=${limit}`);
}

export async function getStats(botId: string): Promise<StatsResponse> {
  return get(`/api/bots/${botId}/stats`);
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

export async function botStart(botId: string): Promise<BotProcessStatus> {
  return post(`/api/bots/${botId}/start`);
}

export async function botStop(botId: string): Promise<BotProcessStatus> {
  return post(`/api/bots/${botId}/stop`);
}

export async function botRestart(botId: string): Promise<BotProcessStatus> {
  return post(`/api/bots/${botId}/restart`);
}
