// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  parseGradientString,
  type RadialCanvas,
  radialGeometry,
  renderRadialGradient,
} from "./gradients.ts";

// The PNG export drew every radial gradient as a centred circle whose radius
// was half the shorter side, whatever shape and position were chosen.

test("radialGeometry places a circle at the named position, sized to the farthest corner", () => {
  const geometry = radialGeometry("circle at top", 1600, 900);

  assert.deepEqual(geometry, {
    cx: 800,
    cy: 0,
    rx: Math.hypot(800, 900),
    ry: Math.hypot(800, 900),
  });
});

test("radialGeometry sizes a centred ellipse to the farthest corner", () => {
  const geometry = radialGeometry("ellipse at center", 1600, 900);

  assert.deepEqual(geometry, {
    cx: 800,
    cy: 450,
    rx: 800 * Math.SQRT2,
    ry: 450 * Math.SQRT2,
  });
});

test("radialGeometry reads a corner position written vertical first", () => {
  const { cx, cy } = radialGeometry("ellipse at top left", 1600, 900);

  assert.deepEqual({ cx, cy }, { cx: 0, cy: 0 });
});

test("radialGeometry puts a percentage after a horizontal keyword on the vertical axis", () => {
  const { cx, cy } = radialGeometry("circle at left 30%", 1600, 900);

  assert.deepEqual({ cx, cy }, { cx: 0, cy: 270 });
});

test("radialGeometry reads pixel positions", () => {
  const { cx, cy } = radialGeometry("circle at 100px 50px", 1600, 900);

  assert.deepEqual({ cx, cy }, { cx: 100, cy: 50 });
});

test("radialGeometry honours closest-side", () => {
  const geometry = radialGeometry("circle closest-side at 25% 50%", 1600, 900);

  assert.deepEqual(geometry, { cx: 400, cy: 450, rx: 400, ry: 400 });
});

test("radialGeometry uses an explicit circle radius", () => {
  const geometry = radialGeometry("circle 100px at center", 1600, 900);

  assert.deepEqual(geometry, { cx: 800, cy: 450, rx: 100, ry: 100 });
});

test("radialGeometry treats a single length as a circle radius", () => {
  const { rx, ry } = radialGeometry("120px", 1600, 900);

  assert.deepEqual({ rx, ry }, { rx: 120, ry: 120 });
});

test("radialGeometry uses explicit ellipse radii, percentages against each axis", () => {
  const { rx, ry } = radialGeometry("ellipse 50% 20%", 1600, 900);

  assert.deepEqual({ rx, ry }, { rx: 800, ry: 180 });
});

test("renderRadialGradient draws the shape radialGeometry computes", () => {
  const calls: unknown[][] = [];
  const canvas: RadialCanvas = {
    fillStyle: "",
    save: () => calls.push(["save"]),
    restore: () => calls.push(["restore"]),
    scale: (...args) => calls.push(["scale", ...args]),
    fillRect: (...args) => calls.push(["fillRect", ...args]),
    createRadialGradient: (...args) => {
      calls.push(["createRadialGradient", ...args]);
      return { addColorStop: () => undefined };
    },
  };
  const parsed = parseGradientString(
    "radial-gradient(ellipse 400px 200px at 25% 50%, #000 0%, #fff 100%)",
  );
  assert.ok(parsed);

  renderRadialGradient(canvas, parsed, 1600, 900);

  // An ellipse is a circle of radius rx in a space squashed by ry / rx.
  assert.deepEqual(calls, [
    ["save"],
    ["scale", 1, 0.5],
    ["createRadialGradient", 400, 900, 0, 400, 900, 400],
    ["fillRect", 0, 0, 1600, 1800],
    ["restore"],
  ]);
});
