// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { palettePresets } from "./palette-presets.ts";

test("palette presets keep at least the original 38 entries", () => {
  assert.ok(palettePresets.length >= 38);
});

test("palette preset names are unique", () => {
  const names = palettePresets.map((preset) => preset.name.toLowerCase());
  assert.equal(new Set(names).size, names.length);
});

test("every palette colour is a #rrggbb hex", () => {
  for (const preset of palettePresets) {
    assert.ok(preset.colors.length > 0, preset.name);
    for (const colour of preset.colors) {
      assert.match(colour, /^#[0-9a-fA-F]{6}$/, `${preset.name}: ${colour}`);
    }
  }
});
