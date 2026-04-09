import { expect, test } from "@playwright/test";

import { installMockWebSocket, stubApi } from "./fixtures";

test.beforeEach(async ({ page }) => {
  await installMockWebSocket(page);
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("__e2e_bootstrapped")) {
      localStorage.clear();
      sessionStorage.clear();
      sessionStorage.setItem("__e2e_bootstrapped", "1");
    }
  });
});

test("login, navigate, and persist the active session", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 1024) < 768, "desktop sidebar navigation");

  await stubApi(page);

  await page.goto("/login");
  await page.getByRole("textbox").first().fill("admin");
  await page.locator('input[type="password"]').fill("secret");
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page).toHaveURL("/");
  await expect(page.getByText("Balance")).toBeVisible();
  await expect(page.getByText("Total PnL")).toBeVisible();

  await page.getByRole("link", { name: "Trades" }).click();
  await expect(page.getByRole("heading", { name: "Trade History" })).toBeVisible();
  await expect(
    page.locator("table").getByRole("cell", { name: "latency-arb" }),
  ).toBeVisible();

  await page.getByRole("link", { name: "Signals" }).click();
  await expect(page.getByRole("heading", { name: "Signal Log" })).toBeVisible();

  await page.getByRole("link", { name: "Logs" }).click();
  await expect(page.getByRole("heading", { name: "Bot Log" })).toBeVisible();
  await expect(page.getByText("connected to feeds")).toBeVisible();

  await page.getByRole("link", { name: "Stats" }).click();
  await expect(page.getByRole("heading", { name: "Bot Status" })).toBeVisible();
  await expect(page.getByText("latency-arb", { exact: true }).first()).toBeVisible();

  await page.reload();
  await expect(page).toHaveURL("/stats");
  await expect(page.getByRole("heading", { name: "Bot Status" })).toBeVisible();
});

test("redirects to login when an authenticated request returns 401", async ({ page }) => {
  await installMockWebSocket(page);
  await page.addInitScript(() => {
    localStorage.setItem("token", "expired-token");
  });

  await page.route("**/api/bots", async (route) => {
    await route.fulfill({
      status: 401,
      contentType: "application/json",
      body: JSON.stringify({ error: "expired" }),
    });
  });

  await page.goto("/");
  await expect(page).toHaveURL("/login");
});
