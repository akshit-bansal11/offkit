import { defineConfig } from "@playwright/test";
import web from "./playwright.config";

// The desktop suite launches the real Electron app against out/, so it needs no web server and no browser project.
export default defineConfig({
  ...web,
  testMatch: "tools.spec.ts",
  use: {},
  projects: [{ name: "desktop" }],
  webServer: undefined,
});
