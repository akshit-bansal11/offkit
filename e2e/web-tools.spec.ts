// The tools on the website: every tool route renders in the browser, and one real job per engine family runs end to end.
import { expect, type Page, test } from "@playwright/test";
import { siteConfig } from "../src/config/site";
import { tools } from "../src/config/tools";
import { pdf, png, wav } from "./fixtures";

// The drop zones hide their <input type="file">; it has no role to find it by.
const upload = "input[type=file]";

// Vercel Web Analytics loads its script from this path, which only Vercel serves.
// Under `next start` it answers 404, so analytics is dark in this suite.
const ANALYTICS_SCRIPT = "/_vercel/insights/";

/** Uncaught errors and console errors the page produces while a test runs. */
function watchErrors(page: Page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    // The 404 is reported against the script's URL, and nosniff's refusal
    // to run the 404 body names that URL in its text.
    const fromAnalytics =
      message.location().url.includes(ANALYTICS_SCRIPT) ||
      message.text().includes(ANALYTICS_SCRIPT);
    if (message.type() === "error" && !fromAnalytics) {
      errors.push(`${message.text()} (${message.location().url})`);
    }
  });
  return errors;
}

// Every test in this file fails if the content security policy blocks
// anything: the page's own violation events and the console's report of them.
let cspViolations: string[] = [];

test.beforeEach(async ({ page }) => {
  cspViolations = [];
  page.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) {
      cspViolations.push(message.text());
    }
  });
  page.on("pageerror", (error) => {
    if (error.message.includes("Content Security Policy")) {
      cspViolations.push(error.message);
    }
  });
  await page.exposeFunction("__reportCspViolation", (report: string) => {
    cspViolations.push(report);
  });
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (event) => {
      const report = Reflect.get(window, "__reportCspViolation");
      if (typeof report === "function") {
        report(`${event.violatedDirective} blocked ${event.blockedURI}`);
      }
    });
  });
});

test.afterEach(() => {
  expect(cspViolations).toEqual([]);
});

test("the CSP violation check catches a blocked request", async ({ page }) => {
  await page.goto("/base64");

  await page.evaluate(() => {
    const image = new Image();
    image.src = "https://example.com/blocked.png";
    document.body.append(image);
  });

  await expect.poll(() => cspViolations.length).toBeGreaterThan(0);
  // This test provokes the violation on purpose.
  cspViolations = [];
});

for (const tool of tools) {
  test(`${tool.href} renders its heading with its own metadata and no errors`, async ({
    page,
  }) => {
    const errors = watchErrors(page);

    const response = await page.goto(tool.href);

    expect(response?.status()).toBe(200);
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
    await expect(
      page.getByRole("heading", { level: 1, name: tool.name, exact: true }),
    ).toBeVisible();
    await expect(page).toHaveTitle(`${tool.name} | open-tools`);
    // Head metadata has no accessible role; the canonical tag is read directly.
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
      "href",
      `${siteConfig.url}${tool.href}`,
    );
    expect(errors).toEqual([]);
  });
}

test.describe("media engine (ffmpeg.wasm)", () => {
  test("audio converter encodes a WAV to MP3", async ({ page }) => {
    const errors = watchErrors(page);
    await page.goto("/audio-converter");

    await page.setInputFiles(upload, wav("tone.wav"));
    await page.getByRole("button", { name: "Convert", exact: true }).click();

    await expect(page.getByText("1 done")).toBeVisible({ timeout: 90_000 });
    expect(errors).toEqual([]);
  });
});

test.describe("image engines (ImageMagick and canvas)", () => {
  test("image converter converts a PNG", async ({ page }) => {
    await page.goto("/image-converter");

    await page.setInputFiles(upload, png("a.png"));
    await page.getByRole("button", { name: "Convert All" }).click();

    await expect(page.getByText("1 converted")).toBeVisible({
      timeout: 60_000,
    });
  });

  test("palette extractor finds both colours", async ({ page }) => {
    await page.goto("/palette-extractor");

    await page.setInputFiles(upload, png("a.png"));

    // Red and blue, plus possibly a blended column where they meet.
    await expect(
      page.getByRole("button", { name: /^#[0-9A-F]{6}/ }).nth(1),
    ).toBeVisible();
  });
});

test.describe("PDF engines (pdf-lib and pdf.js)", () => {
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

  test("pdf splitter names the range parts it cannot use", async ({ page }) => {
    await page.goto("/pdf-splitter");
    await page.setInputFiles(upload, await pdf("a.pdf", 3));

    await page.getByRole("button", { name: "Custom range" }).click();
    await page.getByRole("textbox", { name: "Page range" }).fill("2, 7, x");
    await page.getByRole("button", { name: "Split & Download" }).click();

    await expect(
      page.getByRole("alert").filter({
        hasText:
          'Not a page or range in this 3-page PDF: "7", "x". Use page numbers from 1 to 3, like 1-3, 5.',
      }),
    ).toBeVisible();
  });

  test("pdf to images releases the previous previews when run again", async ({
    page,
  }) => {
    // Records every object URL the page releases.
    await page.addInitScript(() => {
      const released: string[] = [];
      Object.assign(window, { __released: released });
      const revoke = URL.revokeObjectURL.bind(URL);
      URL.revokeObjectURL = (url: string) => {
        released.push(url);
        revoke(url);
      };
    });
    await page.goto("/pdf-converter");
    await page.getByRole("button", { name: "PDF to Images" }).click();
    await page.setInputFiles(upload, await pdf("a.pdf", 2));
    const extract = page.getByRole("button", { name: "Extract Pages" });
    await extract.click();
    const firstPreview = await page
      .getByRole("img", { name: "Page_1.png" })
      .getAttribute("src");

    await extract.click();

    await expect(page.getByText("Extracted Pages (2)")).toBeVisible();
    expect(await page.evaluate("window.__released")).toContain(firstPreview);
  });

  test("pdf compressor offers nothing larger than the original", async ({
    page,
  }) => {
    await page.goto("/pdf-compressor");
    // A one-page text PDF is a few hundred bytes; its rasterised copy is far larger.
    await page.setInputFiles(upload, await pdf("text.pdf", 1));

    await page.getByRole("button", { name: "Compress PDF" }).click();

    await expect(page.getByRole("status")).toContainText(
      "would not be smaller",
    );
    await expect(page.getByRole("button", { name: "Download" })).toHaveCount(0);
  });
});

test.describe("text tools", () => {
  test("json formatter pretty-prints minified JSON", async ({ page }) => {
    await page.goto("/json-formatter");

    await page.getByPlaceholder('{"hello":"world"}').fill('{"a":[1,2]}');
    await page.getByRole("button", { name: "Pretty Print" }).click();

    await expect(page.locator("main pre")).toHaveText(
      '{\n  "a": [\n    1,\n    2\n  ]\n}',
    );
  });

  test("diff checker marks inserted and deleted text", async ({ page }) => {
    await page.goto("/diff-checker");

    await page.getByPlaceholder("Paste the original text here.").fill("cat");
    await page.getByPlaceholder("Paste the modified text here.").fill("cut");

    await expect(page.getByRole("deletion")).toHaveText("a");
    await expect(page.getByRole("insertion")).toHaveText("u");
  });

  test("markdown preview headings sit one level below the page title", async ({
    page,
  }) => {
    await page.goto("/md-editor");

    await expect(
      page.getByRole("heading", {
        level: 2,
        name: "Welcome to Markdown Editor",
      }),
    ).toBeVisible();

    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
  });

  test("base64 round-trips Unicode text", async ({ page }) => {
    await page.goto("/base64");

    await page.locator("textarea").first().fill("héllo 世界");
    await page.getByRole("button", { name: "Encode Text" }).click();

    await expect(page.locator("main")).toContainText("aMOpbGxvIOS4lueVjA==");
  });
});

test.describe("svg tools", () => {
  test("svg animator strips script from an untrusted file", async ({
    page,
  }) => {
    const hostile = [
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" onload="window.__hostile = 1">',
      "<script>window.__hostile = 1</script>",
      '<path d="M1 1 L9 9" onclick="window.__hostile = 1"/>',
      "</svg>",
    ].join("");
    await page.goto("/svg-animator");

    await page.setInputFiles(upload, {
      name: "hostile.svg",
      mimeType: "image/svg+xml",
      buffer: Buffer.from(hostile),
    });

    await expect(page.getByText("Preview", { exact: true })).toBeVisible();
    expect(await page.evaluate("window.__hostile")).toBeUndefined();
  });
});

test.describe("git scaffold (GitHub API, mocked)", () => {
  const TREE = {
    tree: [
      { path: "src", type: "tree", sha: "1" },
      { path: "src/index.ts", type: "blob", sha: "2" },
      { path: "README.md", type: "blob", sha: "3" },
    ],
    truncated: false,
  };

  test("a private repository is read with a token held only in memory", async ({
    page,
  }) => {
    // What an older version left behind: the token in plain text.
    await page.addInitScript(() =>
      window.localStorage.setItem("git_scaffold_token", "ghp_old"),
    );
    const token = "ghp_test_token_value";
    // Every request the page makes, to any host, that carries the token anywhere.
    const carriers: string[] = [];
    page.on("request", (request) => {
      const parts = [
        request.url(),
        request.postData() ?? "",
        ...Object.values(request.headers()),
      ];
      if (parts.some((part) => part.includes(token))) {
        carriers.push(`${request.method()} ${request.url()}`);
      }
    });
    let authorization: string | undefined;
    await page.route("https://api.github.com/**", async (route) => {
      authorization ??= route.request().headers().authorization;
      await route.fulfill({
        json: TREE,
        headers: {
          "access-control-allow-origin": "*",
          "access-control-allow-headers": "authorization, accept",
        },
      });
    });
    await page.goto("/directory-scaffold");

    await page
      .getByRole("textbox", { name: "URL or owner/repo" })
      .fill("octo-org/private-repo");
    await page.getByRole("switch", { name: "Private repository" }).click();
    await page.getByLabel("Personal access token").fill(token);
    await page.getByRole("button", { name: "Fetch tree" }).click();

    await expect(
      page.getByRole("button", { name: /README\.md/ }),
    ).toBeVisible();
    expect(carriers).toEqual([
      "GET https://api.github.com/repos/octo-org/private-repo/git/trees/HEAD?recursive=1",
    ]);
    expect(authorization).toBe(`Bearer ${token}`);
    const stored = await page.evaluate(() =>
      JSON.stringify({ ...localStorage, ...sessionStorage }),
    );
    expect(stored).not.toContain("ghp_");
  });

  test("a private repository without a token gets a clear message", async ({
    page,
  }) => {
    await page.route("https://api.github.com/**", (route) =>
      route.fulfill({
        status: 404,
        json: { message: "Not Found" },
        headers: { "access-control-allow-origin": "*" },
      }),
    );
    await page.goto("/directory-scaffold");

    await page
      .getByRole("textbox", { name: "URL or owner/repo" })
      .fill("octo-org/private-repo");
    await page.getByRole("button", { name: "Fetch tree" }).click();

    // Next renders an empty route announcer with the alert role, so match by text.
    await expect(
      page.getByRole("alert").filter({
        hasText: "If it is private, turn on Private repository",
      }),
    ).toBeVisible();
  });

  test("the rate limit gets a clear message", async ({ page }) => {
    await page.route("https://api.github.com/**", (route) =>
      route.fulfill({
        status: 403,
        json: { message: "API rate limit exceeded" },
        headers: {
          "access-control-allow-origin": "*",
          "access-control-expose-headers": "x-ratelimit-remaining",
          "x-ratelimit-remaining": "0",
        },
      }),
    );
    await page.goto("/directory-scaffold");

    await page
      .getByRole("textbox", { name: "URL or owner/repo" })
      .fill("octo-org/repo");
    await page.getByRole("button", { name: "Fetch tree" }).click();

    await expect(
      page.getByRole("alert").filter({ hasText: "rate limit" }),
    ).toBeVisible();
  });
});
