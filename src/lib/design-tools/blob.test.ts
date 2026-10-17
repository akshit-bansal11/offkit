// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { blobBackground, blobCss, blobTailwind } from "./blob.ts";

const RADIUS = "30% 70% 70% 30% / 30% 30% 70% 70%";

// The preview faded to 25% alpha ("40") while the copied CSS faded to 50%
// ("80"), and the Tailwind output was always violet whatever colour was picked.

test("blobCss copies the same background the preview draws", () => {
  const css = blobCss(RADIUS, "#10b981", true);

  assert.equal(
    css,
    `border-radius: ${RADIUS};\nbackground: ${blobBackground("#10b981", true)};`,
  );
});

test("blobBackground fades the gradient to 50% alpha", () => {
  const background = blobBackground("#10b981", true);

  assert.equal(background, "linear-gradient(135deg, #10b981, #10b98180)");
});

test("blobTailwind uses the picked colour for the gradient", () => {
  const classes = blobTailwind(RADIUS, "#10b981", true);

  assert.equal(
    classes,
    "rounded-[30%_70%_70%_30%_/_30%_30%_70%_70%] bg-[linear-gradient(135deg,#10b981,#10b98180)]",
  );
});

test("blobTailwind uses the picked colour for a flat fill", () => {
  const classes = blobTailwind(RADIUS, "#10b981", false);

  assert.equal(
    classes,
    "rounded-[30%_70%_70%_30%_/_30%_30%_70%_70%] bg-[#10b981]",
  );
});
