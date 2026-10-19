// A UI sweep of every tool page: each visible control can be hit by the
// pointer, each slider moves, and the page renders without errors. Every page
// is screenshotted into ui-sweep/ so CI can upload what it saw.
import { expect, type Page, test } from "@playwright/test";
import { tools } from "../src/config/tools";
import { pdf, png, wav } from "./fixtures";

const CONTROLS =
  "button, a[href], input:not([type=hidden]), select, textarea, [role=slider], [role=switch], [role=tab], [role=checkbox]";

/** Puts a sample in the page's drop zone, if any, so its controls show. */
async function loadSample(page: Page, category: string) {
  const input = page.locator("input[type=file]").first();
  if ((await input.count()) === 0) return;
  if (category === "Images" || category === "Colors") {
    await input.setInputFiles(png("sample.png"));
  } else if (category === "PDF") {
    await input.setInputFiles(await pdf("sample.pdf", 2));
  } else if (category === "Audio") {
    await input.setInputFiles(wav("sample.wav"));
  } else {
    return;
  }
  await page.waitForTimeout(1_500);
}

/** Visible, enabled controls whose centre the pointer cannot reach. */
function coveredControls(page: Page) {
  return page.evaluate((selector) => {
    const covered: string[] = [];
    for (const element of document.querySelectorAll<HTMLElement>(selector)) {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      const hidden =
        rect.width < 2 ||
        rect.height < 2 ||
        style.visibility === "hidden" ||
        style.opacity === "0" ||
        element.closest("[aria-hidden=true], [inert]") !== null;
      if (hidden || (element as HTMLButtonElement).disabled) continue;
      // Native file inputs are hidden on purpose behind their drop zones.
      if (element instanceof HTMLInputElement && element.type === "file") {
        continue;
      }
      element.scrollIntoView({ block: "center", inline: "center" });
      const box = element.getBoundingClientRect();
      const x = box.left + box.width / 2;
      const y = box.top + box.height / 2;
      if (x < 0 || y < 0 || x > innerWidth || y > innerHeight) continue;
      const hit = document.elementFromPoint(x, y);
      const reachable =
        hit !== null &&
        (element.contains(hit) ||
          hit.contains(element) ||
          (hit instanceof HTMLLabelElement && hit.control === element) ||
          hit.closest("label")?.contains(element) === true);
      if (!reachable) {
        const name =
          element.getAttribute("aria-label") ??
          element.textContent?.trim().slice(0, 40) ??
          "";
        const by = hit
          ? `${hit.tagName.toLowerCase()}.${String(hit.className).slice(0, 60)}`
          : "nothing";
        covered.push(
          `<${element.tagName.toLowerCase()}> "${name}" under ${by}`,
        );
      }
    }
    return covered;
  }, CONTROLS);
}

for (const tool of tools) {
  test(`${tool.href} controls are reachable and sliders move`, async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));

    await page.goto(tool.href);
    await loadSample(page, tool.category);

    await page.screenshot({
      path: `ui-sweep/${tool.slug}.png`,
      fullPage: true,
    });

    expect.soft(await coveredControls(page), "covered controls").toEqual([]);

    const sliders = page.locator("input[type=range]:visible:enabled");
    for (let i = 0; i < (await sliders.count()); i += 1) {
      const slider = sliders.nth(i);
      const before = await slider.inputValue();
      const atMax = before === ((await slider.getAttribute("max")) ?? "100");
      await slider.focus();
      await page.keyboard.press(atMax ? "ArrowLeft" : "ArrowRight");
      const after = await slider.inputValue();
      expect.soft(after, `slider ${i} moves`).not.toBe(before);
    }

    expect.soft(errors, "page errors").toEqual([]);
  });
}
