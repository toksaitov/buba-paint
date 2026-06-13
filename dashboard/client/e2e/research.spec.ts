import { expect, test } from "@playwright/test";

import { installMockWebSocket, stubApi, stubResearchApi } from "./fixtures";

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

async function login(page: import("@playwright/test").Page) {
  await stubApi(page);
  await stubResearchApi(page);
  await page.goto("/login");
  await page.getByRole("textbox").first().fill("admin");
  await page.locator('input[type="password"]').fill("secret");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL("/");
}

test("research overview, job creation, job detail, report detail, and compare", async ({
  page,
}) => {
  test.skip(
    (page.viewportSize()?.width ?? 1024) < 768,
    "desktop sidebar navigation",
  );

  await login(page);

  await page.goto("/research");
  await expect(page).toHaveURL("/research");
  await expect(page.getByText("Active jobs")).toBeVisible();
  await expect(page.getByText("Queue cockpit")).toBeVisible();
  await expect(page.getByText("Recent reports")).toBeVisible();

  await page.goto("/research/jobs/new");
  await expect(page.getByText("Job parameters")).toBeVisible();
  await page
    .getByLabel("Artifact to replay")
    .selectOption("fixture-artifact-available");
  await page.getByLabel("Starting balance").fill("200");
  await page.getByRole("button", { name: "Create job" }).click();

  await expect(page).toHaveURL("/research/jobs/fixture-job-created");
});

test("seeded completed job detail shows the step timeline and events", async ({
  page,
}) => {
  test.skip(
    (page.viewportSize()?.width ?? 1024) < 768,
    "desktop sidebar navigation",
  );

  await login(page);
  await page.goto("/research/jobs/fixture-job-completed");

  await expect(page.getByText("Steps (6)")).toBeVisible();
  await expect(page.getByText("Verify artifact")).toBeVisible();
  await expect(page.getByText("Run backtest")).toBeVisible();
  await expect(page.getByText("Write report")).toBeVisible();

  await expect(page.getByText("Events (1)")).toBeVisible();
  await expect(page.getByText("fixture job is completed")).toBeVisible();
});

test("report detail renders schema v2 metrics and the equity chart", async ({
  page,
}) => {
  test.skip(
    (page.viewportSize()?.width ?? 1024) < 768,
    "desktop sidebar navigation",
  );

  await login(page);
  await page.goto("/research/reports/fixture-report-a");

  await expect(page.getByText("Summary metrics")).toBeVisible();
  await expect(page.getByText("Net PnL", { exact: true })).toBeVisible();
  await expect(page.getByText("+$284.25")).toBeVisible();
  await expect(page.getByText("Analysis metrics unavailable")).toHaveCount(0);

  await expect(page.getByText("Equity and drawdown")).toBeVisible();
  await expect(page.getByText("Equity curve")).toBeVisible();
});

test("report compare ranks two reports and warns about mismatched provenance", async ({
  page,
}) => {
  test.skip(
    (page.viewportSize()?.width ?? 1024) < 768,
    "desktop sidebar navigation",
  );

  await login(page);
  await page.goto(
    "/research/reports/compare?ids=fixture-report-a,fixture-report-b",
  );

  await expect(
    page.getByText("Best by Net PnL: Fixture Report fixture-report-a"),
  ).toBeVisible();
  await expect(page.getByText("Compatibility warnings")).toBeVisible();
  await expect(
    page.getByText("Reports use different artifacts."),
  ).toBeVisible();
});
