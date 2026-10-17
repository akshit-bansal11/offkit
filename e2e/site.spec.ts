// The website around the tools: landing page, docs, 404, metadata routes and layout.
import { expect, test } from "@playwright/test";
import { siteConfig } from "../src/config/site";
import { tools } from "../src/config/tools";

const routes = ["/", "/docs", ...tools.map((tool) => tool.href)];

test("landing page offers the download and leads to the docs", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { level: 1, name: "Open Tools" }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Download for Windows" }),
  ).toHaveAttribute(
    "href",
    /\/releases\/latest\/download\/Open-Tools-Setup\.exe$/,
  );
  await page.getByRole("link", { name: "Read the docs" }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Documentation" }),
  ).toBeVisible();
});

test("docs describe every tool", async ({ page }) => {
  await page.goto("/docs");
  await expect(page.getByRole("article")).toHaveCount(tools.length);
});

test("the landing page links to every tool page", async ({ page }) => {
  await page.goto("/");

  const hrefs = await page
    .getByRole("main")
    .getByRole("link")
    .evaluateAll((links) => links.map((link) => link.getAttribute("href")));

  expect(hrefs).toEqual(expect.arrayContaining(tools.map((tool) => tool.href)));
});

test("a docs entry opens its tool", async ({ page }) => {
  await page.goto("/docs");

  await page.getByRole("link", { name: "Open Image Converter" }).click();

  await expect(page).toHaveURL(/\/image-converter$/);
  await expect(
    page.getByRole("heading", { level: 1, name: "Image Converter" }),
  ).toBeVisible();
});

test("a tool URL is served in place, not redirected", async ({ request }) => {
  const response = await request.get("/directory-scaffold", {
    maxRedirects: 0,
  });

  expect(response.status()).toBe(200);
});

test("an unknown path returns 404 with a way back", async ({ page }) => {
  const response = await page.goto("/no-such-page");
  expect(response?.status()).toBe(404);
  await expect(
    page.getByRole("heading", { level: 1, name: "Page not found" }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Go to the home page" }),
  ).toBeVisible();
});

// One table for every security header, so a new header has one place to be registered.
const SECURITY_HEADERS: Record<string, RegExp> = {
  "content-security-policy":
    /default-src 'self'.*script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' blob: .*connect-src 'self' blob: data: https:\/\/api\.github\.com .*object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'/,
  "x-content-type-options": /^nosniff$/,
  "referrer-policy": /^strict-origin-when-cross-origin$/,
  "permissions-policy": /camera=\(\)/,
  "strict-transport-security": /^max-age=63072000; includeSubDomains; preload$/,
};

for (const path of ["/", "/directory-scaffold", "/vendor/pdf.worker.min.mjs"]) {
  test(`${path} is served with the security headers`, async ({ request }) => {
    const response = await request.get(path);

    const headers = response.headers();
    expect(
      Object.fromEntries(
        Object.keys(SECURITY_HEADERS).map((name) => [name, headers[name]]),
      ),
    ).toEqual(
      Object.fromEntries(
        Object.entries(SECURITY_HEADERS).map(([name, pattern]) => [
          name,
          expect.stringMatching(pattern),
        ]),
      ),
    );
  });
}

test("the content security policy allows no eval in production", async ({
  request,
}) => {
  const response = await request.get("/");

  expect(response.headers()["content-security-policy"]).not.toContain(
    "'unsafe-eval'",
  );
});

test("robots.txt, sitemap.xml and the social image are served", async ({
  request,
}) => {
  const robots = await request.get("/robots.txt");
  expect(robots.status()).toBe(200);
  expect(await robots.text()).toContain("Sitemap:");
  const sitemap = await request.get("/sitemap.xml");
  expect(sitemap.status()).toBe(200);
  const urls = await sitemap.text();
  expect(urls).toContain(`<loc>${siteConfig.url}/docs</loc>`);
  expect(urls).toContain(`<loc>${siteConfig.url}/directory-scaffold</loc>`);
  const image = await request.get("/opengraph-image");
  expect(image.status()).toBe(200);
  expect(image.headers()["content-type"]).toContain("image/png");
});

test("the skip link is the first focusable element", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Tab");
  await expect(
    page.getByRole("link", { name: "Skip to content" }),
  ).toBeFocused();
});

for (const route of routes) {
  test(`${route} does not scroll sideways at 320px`, async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 640 });
    await page.goto(route);
    const overflows = await page.evaluate(
      () => document.documentElement.scrollWidth > window.innerWidth,
    );
    expect(overflows).toBe(false);
  });
}

test("the old /git-scaffold URL redirects permanently to /directory-scaffold", async ({
  request,
}) => {
  const response = await request.get("/git-scaffold", { maxRedirects: 0 });

  expect(response.status()).toBe(308);
  expect(response.headers().location).toBe("/directory-scaffold");
});
