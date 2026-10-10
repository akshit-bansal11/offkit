// Smoke tests: one real job per tool family, in a real browser.
import { expect, test } from "@playwright/test";
import { pdf, png, wav } from "./fixtures";

const upload = "input[type=file]";

test("every tool on the home page renders without throwing", async ({
  page,
}) => {
  await page.goto("/");
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
    const response = await page.goto(route);
    expect(response?.status(), route).toBe(200);
    await expect(page.locator("main h1").first(), route).toBeVisible();
  }
  expect(failures).toEqual([]);
});

test("image converter converts a PNG", async ({ page }) => {
  await page.goto("/image-converter");
  await page.setInputFiles(upload, png("a.png"));
  await page.getByRole("button", { name: "Convert All" }).click();
  await expect(page.getByText("1 converted")).toBeVisible({ timeout: 60_000 });
});

test("image cropper produces a circular crop", async ({ page }) => {
  await page.goto("/image-cropper");
  await page.setInputFiles(upload, png("a.png", 1600, 1000));
  await expect(page.getByText("Image list (1)")).toBeVisible();
  await page.getByRole("button", { name: "Circular" }).click();
  await page.getByRole("button", { name: "Preview all" }).click();
  await expect(page.getByAltText("a.png cropped preview")).toBeVisible();
  await expect(page.getByText("Unable to load image")).toHaveCount(0);
});

test("palette extractor finds both colours on-device", async ({ page }) => {
  await page.goto("/palette-extractor");
  await page.setInputFiles(upload, png("a.png"));
  const swatches = page.getByRole("button", { name: /^#[0-9A-F]{6}/ });
  await expect(swatches.first()).toBeVisible();
  // Red and blue, plus possibly a blended column where they meet.
  expect(await swatches.count()).toBeGreaterThanOrEqual(2);
  await expect(page.getByText(/api key/i)).toHaveCount(0);
});

test("pdf merger merges two documents", async ({ page }) => {
  await page.goto("/pdf-merger");
  await page.setInputFiles(upload, [
    await pdf("a.pdf", 3),
    await pdf("b.pdf", 2),
  ]);
  await expect(page.getByText("5 pages total")).toBeVisible();
  await page.getByRole("button", { name: "Merge PDFs" }).click();
  await expect(page.getByText(/Output: .*PDF/)).toBeVisible();
});

test("audio converter encodes Opus with ffmpeg.wasm", async ({ page }) => {
  await page.goto("/audio-converter");
  await page.setInputFiles(upload, wav("a.wav"));
  await page.locator("select").first().selectOption("opus");
  await page.getByRole("button", { name: "Convert", exact: true }).click();
  await expect(page.getByText("1 done")).toBeVisible({ timeout: 90_000 });
});

test("base64 round-trips Unicode text", async ({ page }) => {
  await page.goto("/base64");
  await page.locator("textarea").first().fill("héllo 世界");
  await page.getByRole("button", { name: "Encode Text" }).click();
  await expect(page.locator("main")).toContainText("aMOpbGxvIOS4lueVjA==");
});
