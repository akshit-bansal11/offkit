import { defineConfig } from "@playwright/test";
import web from "./playwright.config";

// The desktop suite launches the real Electron app against out/, so it needs no web server and no browser project.
// It keeps the run settings it had before the web suite moved to one worker and no retries.
export default defineConfig({
  ...web,
  testMatch: "tools.spec.ts",
  fullyParallel: true,
  workers: undefined,
  retries: process.env.CI ? 1 : 0,
  use: {},
  projects: [{ name: "desktop" }],
  webServer: undefined,
});
