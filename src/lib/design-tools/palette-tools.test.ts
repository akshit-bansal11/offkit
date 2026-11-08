// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { colorBlocks } from "./data/color-blocks.ts";
import {
  hueFamily,
  isValidHex,
  lightness,
  moveItem,
  normalizeHex,
  randomHex,
  randomizeColors,
  toCssVariables,
  toJson,
  toTailwindTheme,
} from "./palette-tools.ts";

test("color blocks hold the 639 ported colours, all valid hex", () => {
  assert.equal(colorBlocks.length, 639);
  assert.ok(colorBlocks.every(isValidHex));
});

test("isValidHex accepts only #rrggbb", () => {
  assert.equal(isValidHex("#a1B2c3"), true);
  assert.equal(isValidHex("#abc"), false);
  assert.equal(isValidHex("a1b2c3"), false);
  assert.equal(isValidHex("#a1b2c3d"), false);
  assert.equal(isValidHex("#gggggg"), false);
});

test("normalizeHex trims and lower-cases, or returns null", () => {
  assert.equal(normalizeHex(" #AABBCC "), "#aabbcc");
  assert.equal(normalizeHex("#abc"), null);
});

test("hueFamily names the family of pure and neutral colours", () => {
  assert.equal(hueFamily("#ff0000"), "red");
  assert.equal(hueFamily("#ff8000"), "orange");
  assert.equal(hueFamily("#ffff00"), "yellow");
  assert.equal(hueFamily("#00ff00"), "green");
  assert.equal(hueFamily("#00ffff"), "cyan");
  assert.equal(hueFamily("#0000ff"), "blue");
  assert.equal(hueFamily("#8000ff"), "purple");
  assert.equal(hueFamily("#ff00aa"), "pink");
  assert.equal(hueFamily("#808080"), "neutral");
  assert.equal(hueFamily("#000000"), "neutral");
  assert.equal(hueFamily("#ffffff"), "neutral");
});

test("lightness splits white from black", () => {
  assert.equal(lightness("#ffffff"), "light");
  assert.equal(lightness("#000000"), "dark");
});

test("randomHex is a valid hex for any random source", () => {
  assert.ok(isValidHex(randomHex(() => 0)));
  assert.ok(isValidHex(randomHex(() => 0.999999)));
  assert.ok(isValidHex(randomHex()));
});

test("randomizeColors keeps locked colours and replaces the rest", () => {
  const next = randomizeColors(
    ["#111111", "#222222", "#333333"],
    [true, false, true],
  );

  assert.equal(next[0], "#111111");
  assert.equal(next[2], "#333333");
  assert.notEqual(next[1], "#222222");
  assert.ok(isValidHex(next[1] ?? ""));
});

test("moveItem moves within range and ignores out-of-range targets", () => {
  assert.deepEqual(moveItem(["a", "b", "c"], 0, 1), ["b", "a", "c"]);
  assert.deepEqual(moveItem(["a", "b", "c"], 2, 0), ["c", "a", "b"]);
  assert.deepEqual(moveItem(["a", "b"], 0, -1), ["a", "b"]);
  assert.deepEqual(moveItem(["a", "b"], 1, 2), ["a", "b"]);
});

test("exporters write CSS variables, a Tailwind theme and plain JSON", () => {
  const colors = ["#112233", "#445566"];

  assert.equal(
    toCssVariables(colors),
    ":root {\n  --color-1: #112233;\n  --color-2: #445566;\n}",
  );
  assert.deepEqual(JSON.parse(toTailwindTheme(colors)), {
    theme: {
      extend: { colors: { "palette-1": "#112233", "palette-2": "#445566" } },
    },
  });
  assert.deepEqual(JSON.parse(toJson(colors)), { colors });
});
