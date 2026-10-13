// The website is a landing page and the docs. The tools themselves are not on it.
import { expect, test } from "@playwright/test";
import { tools } from "../src/config/tools";

const routes = ["/", "/docs"];

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

test("an old tool URL lands on its docs entry", async ({ page }) => {
  await page.goto("/image-converter");
  await expect(page).toHaveURL(/\/docs#image-converter$/);
  await expect(
    page.getByRole("heading", {
      level: 4,
      name: "Image Converter",
      exact: true,
    }),
  ).toBeVisible();
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

test("robots.txt, sitemap.xml and the social image are served", async ({
  request,
}) => {
  const robots = await request.get("/robots.txt");
  expect(robots.status()).toBe(200);
  expect(await robots.text()).toContain("Sitemap:");
  const sitemap = await request.get("/sitemap.xml");
  expect(sitemap.status()).toBe(200);
  expect(await sitemap.text()).toContain("/docs");
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
