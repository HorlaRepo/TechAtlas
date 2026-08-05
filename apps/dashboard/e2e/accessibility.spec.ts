import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "./fixtures";

const publicRoutes = [
  "/",
  "/search",
  "/domains?q=nextjs",
  "/domains/example.test",
  "/compare?domains=alpha.test&domains=beta.test",
  "/analytics",
  "/providers/vercel",
  "/technologies",
  "/technologies/nextjs",
  "/about",
];

for (const route of publicRoutes) {
  test(`has no detectable accessibility violations: ${route}`, async ({ page }) => {
    await page.goto(route);
    await expect(page.locator("main")).toBeVisible();
    await expect(new AxeBuilder({ page }).analyze()).resolves.toMatchObject({ violations: [] });
  });
}

test("admin shell has no detectable accessibility violations", async ({ page }) => {
  await page.goto("/admin/overview");
  await expect(page.getByRole("main")).toBeVisible();
  await expect(new AxeBuilder({ page }).analyze()).resolves.toMatchObject({ violations: [] });
});

test("quick-crawl dialog restores focus", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop", "Quick Crawl is in the persistent sidebar at desktop widths.");
  await page.goto("/admin/overview");

  const quickCrawl = page.getByRole("button", { name: "Quick Crawl" });
  await quickCrawl.focus();
  await quickCrawl.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(new AxeBuilder({ page }).include("[role=dialog]").analyze()).resolves.toMatchObject({ violations: [] });
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(quickCrawl).toBeFocused();
});

test("mobile navigation retains a visible keyboard path", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "mobile", "The compact navigation is only visible at mobile widths.");
  await page.goto("/admin/overview");
  const menu = page.getByRole("button", { name: "Open navigation" });
  await menu.focus();
  await menu.press("Enter");
  const navigation = page.getByRole("dialog", { name: "Dashboard navigation" });
  await expect(navigation).toBeVisible();
  await expect(new AxeBuilder({ page }).include("[role=dialog]").analyze()).resolves.toMatchObject({ violations: [] });
  await navigation.getByRole("button", { name: "Close navigation" }).last().click();
  await expect(navigation).toBeHidden();
  await expect(menu).toBeFocused();
});

test("analytics loading, empty, and reduced-motion states remain accessible", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  let releaseRequests: (() => void) | undefined;
  const heldRequests = new Promise<void>((resolve) => {
    releaseRequests = resolve;
  });
  await page.route("**/api/v1/public/analytics/**", async (route) => {
    await heldRequests;
    await route.fallback();
  });

  const navigation = page.goto("/analytics");
  await expect(page.getByText("Loading intelligence analytics")).toBeVisible();
  releaseRequests?.();
  await navigation;
  await expect(page.getByRole("img", { name: "Line chart showing daily technology adoption counts." })).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.matchMedia("(prefers-reduced-motion: reduce)").matches)).toBe(true);

  await page.goto("/analytics?since_days=365");
  await expect(page.getByText("Insufficient history for a trend.")).toBeVisible();
  await expect(page.getByText("No changes were observed in this window.")).toBeVisible();
  await expect(new AxeBuilder({ page }).analyze()).resolves.toMatchObject({ violations: [] });
});

test("analytics error state is announced accessibly", async ({ page }) => {
  await page.route("**/api/v1/public/analytics/overview", async (route) => {
    await route.fulfill({ status: 503, contentType: "application/json", body: JSON.stringify({ error: { code: "unavailable", message: "fixture" } }) });
  });
  await page.goto("/analytics");
  await expect(page.getByText("Could not load analytics")).toBeVisible();
  await expect(new AxeBuilder({ page }).analyze()).resolves.toMatchObject({ violations: [] });
});
