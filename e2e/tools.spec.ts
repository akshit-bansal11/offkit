// Smoke tests: one real job per tool family, in the real Electron app.
import {
  type ElectronApplication,
  _electron as electron,
  expect,
  type Page,
  test,
} from "@playwright/test";
import { pdf, png, wav } from "./fixtures";

const APP = "app://open-tools";
const upload = "input[type=file]";

let app: ElectronApplication;
let page: Page;

// Things the app must never do: reach the network, or trip its own content security policy.
const violations: string[] = [];

test.beforeEach(async ({}, testInfo) => {
  violations.length = 0;
  // A throwaway profile per test, so runs never share state or touch a real one.
  app = await electron.launch({
    args: [".", `--user-data-dir=${testInfo.outputPath("profile")}`],
  });
  page = await app.firstWindow();
  page.on("request", (request) => {
    if (request.url().startsWith("http")) violations.push(request.url());
  });
  page.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) {
      violations.push(message.text());
    }
  });
});

test.afterEach(async () => {
  await app.close();
  expect(violations).toEqual([]);
});

test("every tool on the home page renders without throwing", async () => {
  await page.goto(`${APP}/`);
  const routes = await page
    .locator('main a[href^="/"]')
    .evaluateAll((links) => [
      ...new Set(links.map((link) => link.getAttribute("href") ?? "")),
    ]);
  expect(routes.length).toBeGreaterThanOrEqual(37);
  expect(routes).toEqual(
    expect.arrayContaining(["/image-cropper", "/pdf-toolkit"]),
  );

  const failures: string[] = [];
  page.on("pageerror", (error) =>
    failures.push(`${page.url()}: ${error.message}`),
  );

  for (const route of routes) {
    const response = await page.goto(`${APP}${route}`);
    expect(response?.status(), route).toBe(200);
    await expect(page.locator("main h1").first(), route).toBeVisible();
  }
  expect(failures).toEqual([]);
});

test("image converter converts a PNG", async () => {
  await page.goto(`${APP}/image-converter`);
  await page.setInputFiles(upload, png("a.png"));
  await page.getByRole("button", { name: "Convert All" }).click();
  await expect(page.getByText("1 converted")).toBeVisible({ timeout: 60_000 });
});

test("image cropper produces a circular crop", async () => {
  await page.goto(`${APP}/image-cropper`);
  await page.setInputFiles(upload, png("a.png", 1600, 1000));
  await expect(page.getByText("Image list (1)")).toBeVisible();
  await page.getByRole("button", { name: "Circular" }).click();
  await page.getByRole("button", { name: "Preview all" }).click();
  await expect(page.getByAltText("a.png cropped preview")).toBeVisible();
  await expect(page.getByText("Unable to load image")).toHaveCount(0);
});

test("palette extractor finds both colours on-device", async () => {
  await page.goto(`${APP}/palette-extractor`);
  await page.setInputFiles(upload, png("a.png"));
  const swatches = page.getByRole("button", { name: /^#[0-9A-F]{6}/ });
  await expect(swatches.first()).toBeVisible();
  // Red and blue, plus possibly a blended column where they meet.
  expect(await swatches.count()).toBeGreaterThanOrEqual(2);
  await expect(page.getByText(/api key/i)).toHaveCount(0);
});

test("pdf merger merges two documents", async () => {
  await page.goto(`${APP}/pdf-merger`);
  await page.setInputFiles(upload, [
    await pdf("a.pdf", 3),
    await pdf("b.pdf", 2),
  ]);
  await expect(page.getByText("5 pages total")).toBeVisible();
  await page.getByRole("button", { name: "Merge PDFs" }).click();
  await expect(page.getByText(/Output: .*PDF/)).toBeVisible();
});

test("audio converter encodes Opus with ffmpeg.wasm", async () => {
  await page.goto(`${APP}/audio-converter`);
  await page.setInputFiles(upload, wav("a.wav"));
  await page.locator("select").first().selectOption("opus");
  await page.getByRole("button", { name: "Convert", exact: true }).click();
  await expect(page.getByText("1 done")).toBeVisible({ timeout: 90_000 });
});

test("base64 round-trips Unicode text", async () => {
  await page.goto(`${APP}/base64`);
  await page.locator("textarea").first().fill("héllo 世界");
  await page.getByRole("button", { name: "Encode Text" }).click();
  await expect(page.locator("main")).toContainText("aMOpbGxvIOS4lueVjA==");
});

test("pages are served with a content security policy", async () => {
  const response = await page.goto(`${APP}/`);
  expect(response?.headers()["content-security-policy"]).toContain(
    "default-src 'self'",
  );
});

test("an unknown path answers 404", async () => {
  const response = await page.goto(`${APP}/no-such-page`);
  expect(response?.status()).toBe(404);
});

test("the app refuses to reach the network", async () => {
  await page.goto(`${APP}/`);
  const outcome = await page.evaluate(() =>
    fetch("https://example.com").then(
      () => "reached",
      () => "blocked",
    ),
  );
  expect(outcome).toBe("blocked");
  // This test provokes the violation on purpose.
  violations.length = 0;
});
