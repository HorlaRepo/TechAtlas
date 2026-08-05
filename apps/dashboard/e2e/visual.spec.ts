import { expect, test } from "./fixtures";

const visualRoutes = [
  ["home", "/"],
  ["dense-domain-search", "/domains?q=nextjs&sort=updated_desc"],
  ["domain-profile", "/domains/example.test"],
  ["analytics-chart", "/analytics"],
  ["analytics-empty-chart", "/analytics?since_days=365"],
  ["admin-overview", "/admin/overview"],
] as const;

for (const [name, route] of visualRoutes) {
  test(`${name} matches the approved visual baseline`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto(route);
    await expect(page.locator("main")).toBeVisible();
    if (name === "analytics-chart") {
      await expect(page.getByRole("img", { name: "Line chart showing daily technology adoption counts." })).toBeVisible();
    }
    await expect(page).toHaveScreenshot(`${name}.png`, { animations: "disabled", fullPage: true });
  });
}
