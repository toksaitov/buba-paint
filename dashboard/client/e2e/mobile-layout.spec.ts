import { expect, test } from "@playwright/test";

import { installMockWebSocket, stubApi } from "./fixtures";

function isMobileViewport(width?: number) {
  return (width ?? 1024) < 768;
}

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

test("mobile drawer opens and closes", async ({ page }) => {
  test.skip(!isMobileViewport(page.viewportSize()?.width), "mobile only");

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
  await expect(page.getByRole("link", { name: "Trades", exact: true })).toBeVisible();

  await page.getByRole("link", { name: "Trades", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Trade history" })).toBeVisible();

  await expect(page.getByRole("link", { name: "Trades", exact: true })).not.toBeVisible();
});

test("mobile shows trade cards instead of table", async ({ page }) => {
  test.skip(!isMobileViewport(page.viewportSize()?.width), "mobile only");

  await stubApi(page);
  await page.goto("/login");
  await page.getByRole("textbox").first().fill("admin");
  await page.locator('input[type="password"]').fill("secret");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByText("Balance", { exact: true })).toBeVisible();

  const hamburger = page.getByRole("button", { name: "Expand navigation" });
  await hamburger.click({ force: true });
  await page.getByRole("link", { name: "Trades", exact: true }).click();

  await expect(
    page.locator("span:visible").filter({ hasText: /^latency-arb$/ }).first(),
  ).toBeVisible();
  await expect(page.locator("table")).not.toBeVisible();
});

test("mobile standalone shell keeps toolbar below safe area", async ({ page }) => {
  test.skip(!isMobileViewport(page.viewportSize()?.width), "mobile only");

  await page.addInitScript(() => {
    Object.defineProperty(navigator, "standalone", {
      value: true,
      configurable: true,
    });
    const originalMatchMedia = window.matchMedia.bind(window);
    window.matchMedia = (query: string) => {
      if (query === "(display-mode: standalone)") {
        return {
          matches: true,
          media: query,
          onchange: null,
          addEventListener: () => {},
          removeEventListener: () => {},
          addListener: () => {},
          removeListener: () => {},
          dispatchEvent: () => false,
        };
      }
      return originalMatchMedia(query);
    };
  });

  await stubApi(page);
  await page.goto("/login");
  await page.getByRole("textbox").first().fill("admin");
  await page.locator('input[type="password"]').fill("secret");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL("/");
  await page.addStyleTag({
    content: ":root { --app-safe-top: 47px; --app-safe-right: 0px; --app-safe-bottom: 21px; --app-safe-left: 0px; }",
  });

  await expect(page.locator("html")).toHaveClass(/standalone/);
  const headerBox = await page.getByRole("banner").boundingBox();
  const menuBox = await page.getByRole("button", { name: "Expand navigation" }).boundingBox();
  expect(headerBox?.height ?? 0).toBeGreaterThanOrEqual(100);
  expect(menuBox?.y ?? 0).toBeGreaterThanOrEqual(47);
  await page.getByRole("button", { name: "More header controls" }).click();
  await expect(page.getByRole("menu")).toBeVisible();
  await expect(page.getByText("Start bot", { exact: true })).toBeVisible();
});
