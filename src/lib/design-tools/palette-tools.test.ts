// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { colorBlocks } from "./data/color-blocks.ts";
import {
  type HueFamily,
  hslToHex,
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

// A small seeded generator, so the random tests repeat exactly.
function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test("randomizeColors keeps locked colours and replaces the rest", () => {
  const next = randomizeColors(
    ["#111111", "#222222", "#333333"],
    [true, false, true],
    seeded(1),
  );

  assert.equal(next[0], "#111111");
  assert.equal(next[2], "#333333");
  assert.notEqual(next[1], "#222222");
  assert.ok(isValidHex(next[1] ?? ""));
});

test("randomizeColors gives the same colours for the same seed", () => {
  const colors = ["#111111", "#222222", "#333333"];
  const locked = [false, false, false];

  assert.deepEqual(
    randomizeColors(colors, locked, seeded(7)),
    randomizeColors(colors, locked, seeded(7)),
  );
  assert.notDeepEqual(
    randomizeColors(colors, locked, seeded(7)),
    randomizeColors(colors, locked, seeded(8)),
  );
});

test("randomizeColors changes nothing when every colour is locked", () => {
  const colors = ["#111111", "#222222"];

  assert.deepEqual(randomizeColors(colors, [true, true], seeded(3)), colors);
});

test("moveItem moves within range and ignores out-of-range targets", () => {
  assert.deepEqual(moveItem(["a", "b", "c"], 0, 1), ["b", "a", "c"]);
  assert.deepEqual(moveItem(["a", "b", "c"], 2, 0), ["c", "a", "b"]);
  assert.deepEqual(moveItem(["a", "b"], 0, -1), ["a", "b"]);
  assert.deepEqual(moveItem(["a", "b"], 1, 2), ["a", "b"]);
});

test("exporters write CSS variables, a Tailwind v4 theme and plain JSON", () => {
  const colors = ["#112233", "#445566"];

  assert.equal(
    toCssVariables(colors),
    ":root {\n  --color-1: #112233;\n  --color-2: #445566;\n}",
  );
  assert.equal(
    toTailwindTheme(colors),
    "@theme {\n  --color-palette-1: #112233;\n  --color-palette-2: #445566;\n}",
  );
  assert.deepEqual(JSON.parse(toJson(colors)), { colors });
});

// Hue boundaries sit at 15, 45, 70, 165, 200, 255, 290 and 350 degrees. Each
// is probed a degree either side, at full saturation and mid lightness.
const hueBoundaries: [number, HueFamily, HueFamily][] = [
  [15, "red", "orange"],
  [45, "orange", "yellow"],
  [70, "yellow", "green"],
  [165, "green", "cyan"],
  [200, "cyan", "blue"],
  [255, "blue", "purple"],
  [290, "purple", "pink"],
  [350, "pink", "red"],
];

for (const [boundary, below, above] of hueBoundaries) {
  test(`hueFamily switches from ${below} to ${above} at ${boundary} degrees`, () => {
    assert.equal(hueFamily(hslToHex(boundary - 1, 1, 0.5)), below);
    assert.equal(hueFamily(hslToHex(boundary + 1, 1, 0.5)), above);
  });
}

test("hueFamily calls low saturation and extreme lightness neutral", () => {
  assert.equal(hueFamily(hslToHex(0, 0.11, 0.5)), "neutral");
  assert.equal(hueFamily(hslToHex(0, 0.13, 0.5)), "red");
  assert.equal(hueFamily(hslToHex(0, 1, 0.07)), "neutral");
  assert.equal(hueFamily(hslToHex(0, 1, 0.09)), "red");
  assert.equal(hueFamily(hslToHex(0, 1, 0.96)), "neutral");
  assert.equal(hueFamily(hslToHex(0, 1, 0.94)), "red");
});

// The same fixture is read by the desktop's Rust test, so the two cannot drift.
interface ParityEntry {
  hex: string;
  family: string;
  lightness: string;
}

function isParityEntry(value: unknown): value is ParityEntry {
  return (
    typeof value === "object" &&
    value !== null &&
    "hex" in value &&
    typeof value.hex === "string" &&
    "family" in value &&
    typeof value.family === "string" &&
    "lightness" in value &&
    typeof value.lightness === "string"
  );
}

test("family and lightness match the shared web and desktop fixture", () => {
  const raw: unknown = JSON.parse(
    readFileSync(new URL("./data/color-parity.json", import.meta.url), "utf8"),
  );
  const entries = Array.isArray(raw) ? raw.filter(isParityEntry) : [];

  assert.ok(entries.length >= 20);
  for (const entry of entries) {
    assert.equal(hueFamily(entry.hex), entry.family, entry.hex);
    assert.equal(lightness(entry.hex), entry.lightness, entry.hex);
  }
});
