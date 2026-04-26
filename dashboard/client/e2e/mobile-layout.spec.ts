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

test("mobile drawer opens and closes", async ({ page, browserName }) => {
  test.skip(browserName !== "webkit" && !page.viewportSize()?.width || (page.viewportSize()?.width ?? 1024) >= 768, "mobile only");

  await stubApi(page);
  await page.goto("/login");
  await page.getByRole("textbox").first().fill("admin");
  await page.locator('input[type="password"]').fill("secret");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL("/");

  await expect(page.getByText("Balance", { exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Current market" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Open shadow trades" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Polymarket account" })).toBeVisible();

  const hamburger = page.getByRole("button", { name: "Expand navigation" });
  await expect(hamburger).toBeVisible();
  await expect(hamburger).toBeEnabled();

  await hamburger.click({ force: true });
  await expect(page.getByRole("link", { name: "Trades" })).toBeVisible();

  await page.getByRole("link", { name: "Trades" }).click();
  await expect(page.getByRole("heading", { name: "Trade history" })).toBeVisible();

  await expect(page.getByRole("link", { name: "Trades" })).not.toBeVisible();
});

test("mobile shows trade cards instead of table", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 1024) >= 768, "mobile only");

  await stubApi(page);
  await page.goto("/login");
  await page.getByRole("textbox").first().fill("admin");
  await page.locator('input[type="password"]').fill("secret");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByText("Balance", { exact: true })).toBeVisible();

  const hamburger = page.getByRole("button", { name: "Expand navigation" });
  await hamburger.click({ force: true });
  await page.getByRole("link", { name: "Trades" }).click();

  await expect(
    page.locator("span:visible").filter({ hasText: /^latency-arb$/ }).first(),
  ).toBeVisible();
  await expect(page.locator("table")).not.toBeVisible();
});
