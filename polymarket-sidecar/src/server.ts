import http from "node:http";
import { loadConfig, type SidecarConfig } from "./config.js";
import {
  ProviderStageError,
  createDefaultProvider,
  type SidecarProvider,
} from "./provider.js";
import type {
  LiveOrderIntentRequest,
  LivePreflightRequest,
} from "./types.js";

function json(
  res: http.ServerResponse,
  statusCode: number,
  payload: unknown,
): void {
  res.statusCode = statusCode;
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify(payload));
}

async function readBody(req: http.IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}

function badRequest(res: http.ServerResponse, error: string): void {
  json(res, 400, { error });
}

function parseJson<T>(body: string): T {
  if (body.trim().length === 0) {
    throw new Error("request body must be valid JSON");
  }
  try {
    return JSON.parse(body) as T;
  } catch {
    throw new Error("request body must be valid JSON");
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function validatePreflightRequest(value: unknown): value is LivePreflightRequest {
  if (!isRecord(value) || !isRecord(value.budget_limits)) return false;
  const budget = value.budget_limits;
  return (
    (value.execution_mode === "live_readonly" || value.execution_mode === "live_trading") &&
    isNonEmptyString(value.clob_api_url) &&
    isNonEmptyString(value.gamma_api_url) &&
    Array.isArray(value.strategy_readiness) &&
    isFiniteNumber(budget.cash_cap_usd) &&
    isFiniteNumber(budget.max_single_order_usd) &&
    isFiniteNumber(budget.max_open_notional_usd) &&
    isFiniteNumber(budget.max_daily_loss_usd) &&
    isFiniteNumber(budget.max_session_drawdown_usd) &&
    isFiniteNumber(budget.min_required_cash_usd)
  );
}

function validateOrderIntent(value: unknown): value is LiveOrderIntentRequest {
  return (
    isRecord(value) &&
    isFiniteNumber(value.session_id) &&
    isFiniteNumber(value.intent_id) &&
    isNonEmptyString(value.market_id) &&
    isNonEmptyString(value.token_id) &&
    isNonEmptyString(value.side) &&
    isNonEmptyString(value.order_type) &&
    isFiniteNumber(value.limit_price) &&
    isFiniteNumber(value.size) &&
    (value.amount_usd == null || isFiniteNumber(value.amount_usd)) &&
    isNonEmptyString(value.client_order_id)
  );
}

export function createServer(
  provider: SidecarProvider = createDefaultProvider(loadConfig()),
  config: SidecarConfig = loadConfig(),
): http.Server {
  let providerClosePromise: Promise<void> | null = null;
  const closeProvider = async (): Promise<void> => {
    if (!providerClosePromise) {
      providerClosePromise = Promise.resolve(provider.close()).then(() => undefined);
    }
    return providerClosePromise;
  };

  const server = http.createServer(async (req, res) => {
    try {
      if (!req.url || !req.method) {
        json(res, 400, { error: "invalid request" });
        return;
      }

      if (req.method === "GET" && req.url === "/health") {
        json(res, 200, await provider.health());
        return;
      }

      if (req.method === "GET" && req.url === "/account") {
        json(res, 200, await provider.accountState());
        return;
      }

      if (req.method === "GET" && req.url === "/activity") {
        json(res, 200, await provider.activity());
        return;
      }

      if (req.method === "POST" && req.url === "/preflight") {
        const body = parseJson<unknown>(await readBody(req));
        if (!validatePreflightRequest(body)) {
          badRequest(res, "invalid preflight request");
          return;
        }
        json(res, 200, await provider.preflight(body));
        return;
      }

      if (req.method === "POST" && req.url === "/orders") {
        const body = parseJson<unknown>(await readBody(req));
        if (!validateOrderIntent(body)) {
          badRequest(res, "invalid order intent request");
          return;
        }
        json(res, 200, await provider.submitOrderIntent(body));
        return;
      }

      if (req.method === "POST" && req.url === "/cancel") {
        const body = parseJson<unknown>(await readBody(req));
        if (!isRecord(body) || !isNonEmptyString(body.order_id)) {
          badRequest(res, "order_id is required");
          return;
        }
        json(res, 200, await provider.cancelOrder(body.order_id));
        return;
      }

      if (req.method === "POST" && req.url === "/cancel-all") {
        json(res, 200, await provider.cancelAll());
        return;
      }

      if (req.method === "POST" && req.url === "/redeem-all") {
        json(res, 200, await provider.redeemAll());
        return;
      }

      json(res, 404, { error: "not found" });
    } catch (error) {
      const message = error instanceof Error ? error.message : "internal error";
      if (message === "request body must be valid JSON") {
        badRequest(res, message);
        return;
      }
      if (error instanceof ProviderStageError) {
        json(res, 503, { error: error.message });
        return;
      }
      json(res, 500, { error: message });
    }
  });
  server.once("close", () => {
    void closeProvider();
  });
  return server;
}
