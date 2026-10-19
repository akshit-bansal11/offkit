import { defineConfig, devices } from "@playwright/test";

// The website suite: landing page, docs and every tool, served by `next start`.
export default defineConfig({
  testDir: "e2e",
  testMatch: ["site.spec.ts", "web-tools.spec.ts", "ui-sweep.spec.ts"],
  timeout: 120_000,
  expect: { timeout: 20_000 },
  // One worker: the media tests load a 32 MB wasm engine per page, and the
  // machine this runs on may be compiling the native app at the same time.
  workers: 1,
  forbidOnly: !!process.env.CI,
  // A retry turns a defect into a green run.
  retries: 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:3000",
    trace: "retain-on-failure",
    video: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "web", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "npm run build && npm run start",
    url: "http://localhost:3000",
    timeout: 300_000,
    reuseExistingServer: !process.env.CI,
  },
});
