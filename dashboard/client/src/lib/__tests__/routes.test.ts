import { describe, expect, test } from "vitest";
import { dashboardRoutes, routeMetaForPath } from "../routes";

describe("dashboardRoutes", () => {
  test("keeps Execution near the top under Monitor and Research at the end", () => {
    expect(dashboardRoutes.map((route) => route.label)).toEqual([
      "Overview",
      "Execution",
      "Logs",
      "Parameters",
      "Machine",
      "Trend",
      "Trades",
      "Signals",
      "Strategies",
      "Overview",
      "Machines",
      "Artifacts",
      "Transfers",
      "Jobs",
      "Reports",
    ]);
    expect(dashboardRoutes[1]?.section).toBe("Monitor");
    expect(dashboardRoutes[dashboardRoutes.length - 1]?.section).toBe("Research");
  });

  test("intro bar shows on Monitor entries and all Research entries", () => {
    expect(
      dashboardRoutes
        .filter((route) => route.showContextStrip)
        .map((route) => route.label),
    ).toEqual([
      "Overview",
      "Execution",
      "Logs",
      "Parameters",
      "Machine",
      "Overview",
      "Machines",
      "Artifacts",
      "Transfers",
      "Jobs",
      "Reports",
    ]);
  });
});

describe("routeMetaForPath", () => {
  test("maps compatibility routes to Execution", () => {
    const route = routeMetaForPath("/live");
    expect(route.label).toBe("Execution");
    expect(route.to).toBe("/execution");
    expect(routeMetaForPath("/trading").label).toBe("Execution");
  });

  test("maps compatibility stats route to Strategies", () => {
    const route = routeMetaForPath("/stats");
    expect(route.label).toBe("Strategies");
    expect(route.to).toBe("/strategies");
  });

  test("matches nested analysis routes", () => {
    const route = routeMetaForPath("/trades/123");
    expect(route.label).toBe("Trades");
    expect(route.scope).toBe("shadow");
  });

  test("falls back to overview for unknown paths", () => {
    const route = routeMetaForPath("/unknown");
    expect(route.label).toBe("Overview");
    expect(route.scope).toBe("mixed");
  });
});
