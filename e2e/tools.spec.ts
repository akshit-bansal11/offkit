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

let app: ElectronApplication | undefined;
let page: Page;

// Things the app must never do: reach the network, or trip its own content security policy.
const violations: string[] = [];

test.beforeEach(async ({}, testInfo) => {
  violations.length = 0;
  app = undefined;
  // A throwaway profile per test, so runs never share state or touch a real one.
  const profile = `--user-data-dir=${testInfo.outputPath("profile")}`;
  // CI sets this to the packaged app, so the tests cover what ships (served out of app.asar), not only the dev tree.
  const packaged = process.env.OPEN_TOOLS_EXE;
  const launched = await electron.launch(
    packaged
      ? { executablePath: packaged, args: [profile] }
      : { args: [".", profile] },
  );
  app = launched;
  page = await launched.firstWindow();
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
  await app?.close();
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

test("only the diff checker may read the clipboard", async () => {
  // A string expression, because TypeScript's PermissionName does not list clipboard-read.
  const clipboardRead =
    "navigator.permissions.query({ name: 'clipboard-read' }).then((status) => status.state)";

  await page.goto(`${APP}/`);
  await page.getByRole("link", { name: "Diff", exact: true }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Diff Checker" }),
  ).toBeVisible();
  expect(await page.evaluate(clipboardRead)).toBe("granted");

  await page.goto(`${APP}/base64`);
  expect(await page.evaluate(clipboardRead)).toBe("denied");
});

test("GitHub is reachable only for a repository tree read", async () => {
  await page.goto(`${APP}/`);
  // Runs in the page: resolves "reached" for any HTTP answer and "blocked" when the request is cancelled.
  const attempt = (url: string, method: string) =>
    page.evaluate(
      ([target, verb]) =>
        fetch(target, { method: verb }).then(
          () => "reached",
          () => "blocked",
        ),
      [url, method],
    );

  // Live integration: this one call really goes to GitHub.
  expect(
    await attempt(
      "https://api.github.com/repos/akshit-bansal11/open-tools/git/trees/HEAD",
      "GET",
    ),
  ).toBe("reached");
  expect(await attempt("https://api.github.com/gists", "POST")).toBe("blocked");
  expect(await attempt("https://api.github.com/user", "GET")).toBe("blocked");
  // This test provokes the violations on purpose.
  violations.length = 0;
});

test("svg animator strips script from an untrusted file", async () => {
  const hostile = [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" onload="window.__hostile = 1">',
    "<script>window.__hostile = 1</script>",
    '<path d="M1 1 L9 9" onclick="window.__hostile = 1"/>',
    "<title>a&#160;b</title>",
    "</svg>",
  ].join("");

  await page.goto(`${APP}/svg-animator`);
  await page.setInputFiles(upload, {
    name: "hostile.svg",
    mimeType: "image/svg+xml",
    buffer: Buffer.from(hostile),
  });

  // The file still loads: sanitising must not break valid XML.
  await expect(page.getByText("Preview", { exact: true })).toBeVisible();
  await expect(page.getByText("This SVG could not be parsed.")).toHaveCount(0);
  expect(await page.evaluate("window.__hostile")).toBeUndefined();
});
