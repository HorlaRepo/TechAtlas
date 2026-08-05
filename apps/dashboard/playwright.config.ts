import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";

const dashboardRoot = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  workers: 2,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://127.0.0.1:4173",
    colorScheme: "dark",
    reducedMotion: "reduce",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 1000 }, reducedMotion: "reduce" } },
    { name: "tablet", use: { ...devices["iPad (gen 7)"], viewport: { width: 768, height: 1024 }, browserName: "chromium", reducedMotion: "reduce" } },
    { name: "mobile", use: { ...devices["iPhone 13"], viewport: { width: 390, height: 844 }, browserName: "chromium", reducedMotion: "reduce" } },
  ],
  webServer: {
    command: "VITE_AUTH0_DOMAIN=e2e.auth0.invalid VITE_AUTH0_CLIENT_ID=e2e-client VITE_AUTH0_AUDIENCE=https://e2e.invalid VITE_E2E_TEST_MODE=true pnpm build && VITE_AUTH0_DOMAIN=e2e.auth0.invalid VITE_AUTH0_CLIENT_ID=e2e-client VITE_AUTH0_AUDIENCE=https://e2e.invalid VITE_E2E_TEST_MODE=true pnpm exec vite preview --host 127.0.0.1 --port 4173 --strictPort",
    cwd: dashboardRoot,
    url: "http://127.0.0.1:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
