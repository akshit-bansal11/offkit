// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { gradientPresets } from "./gradient-presets.ts";

test("gradient presets keep at least the original 47 entries", () => {
  assert.ok(gradientPresets.length >= 47);
});

test("gradient preset names are unique", () => {
  const names = gradientPresets.map((preset) => preset.name.toLowerCase());
  assert.equal(new Set(names).size, names.length);
});

test("every gradient stop is a #rrggbb hex", () => {
  for (const preset of gradientPresets) {
    const stops = preset.css.match(/#[0-9a-zA-Z]+/g) ?? [];
    assert.ok(stops.length >= 2, preset.name);
    for (const stop of stops) {
      assert.match(stop, /^#[0-9a-fA-F]{6}$/, `${preset.name}: ${stop}`);
    }
  }
});
