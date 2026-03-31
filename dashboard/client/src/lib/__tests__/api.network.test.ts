import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";

import { server } from "../../test/msw-server";
import { getBots, getTrades } from "../api";

describe("api module over real fetch with MSW", () => {
  beforeAll(() => {
    server.listen({ onUnhandledRequest: "error" });
  });

  afterEach(() => {
    server.resetHandlers();
  });

  afterAll(() => {
    server.close();
  });

  beforeEach(() => {
    localStorage.clear();
  });

  test("getBots sends the bearer token header", async () => {
    localStorage.setItem("token", "jwt-123");
    let seenAuth: string | null = null;

    server.use(
      http.get("/api/bots", ({ request }) => {
        seenAuth = request.headers.get("authorization");
        return HttpResponse.json({
          bots: [{ id: "paint", name: "Paint" }],
        });
      }),
    );

    await expect(getBots()).resolves.toEqual({
      bots: [{ id: "paint", name: "Paint" }],
    });
    expect(seenAuth).toBe("Bearer jwt-123");
  });

  test("getTrades preserves additive execution fields from the server", async () => {
    server.use(
      http.get("/api/bots/:botId/trades", () =>
        HttpResponse.json({
          trades: [
            {
              id: 1,
              timestamp: 1716000000000,
              market_id: "mkt-1",
              strategy: "latency-arb",
              side: "UP",
              token_id: "tok-up",
              size: 10,
              entry_price: 0.55,
              status: "closed",
              pnl: 5.5,
              settlement_price: 1,
              resolved_at: 1716000100000,
              fill_status: "filled",
              execution_group_id: "spread-1",
              execution_fidelity: "legacy_snapshot",
              filled_size: 10,
              avg_fill_price: 0.55,
            },
          ],
          total: 1,
          page: 1,
          per_page: 50,
        }),
      ),
    );

    const resp = await getTrades("paint");
    expect(resp.trades[0].fill_status).toBe("filled");
    expect(resp.trades[0].execution_group_id).toBe("spread-1");
    expect(resp.trades[0].execution_fidelity).toBe("legacy_snapshot");
    expect(resp.trades[0].filled_size).toBe(10);
    expect(resp.trades[0].avg_fill_price).toBe(0.55);
  });
});
