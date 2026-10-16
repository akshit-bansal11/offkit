// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { toolCategories, tools } from "../config/tools.ts";
import { quantizePixels } from "./design-tools/palette-extractor.ts";
import {
  audioCodecArgs,
  describeFFmpegFailure,
  VIDEO_FORMAT_CODEC_MAP,
  videoContainerCodecArgs,
} from "./ffmpeg/client.ts";
import { parseRanges } from "./tools/pdf-toolkit/utils.ts";

function pixels(...runs: Array<[number, number, number, number, number]>) {
  return Uint8Array.from(
    runs.flatMap(([count, r, g, b, a]) =>
      Array.from({ length: count }, () => [r, g, b, a]).flat(),
    ),
  );
}

test("quantizePixels orders colours by dominance and skips transparent pixels", () => {
  const data = pixels(
    [60, 255, 0, 0, 255],
    [30, 0, 0, 255, 255],
    [10, 0, 255, 0, 255],
    [500, 9, 9, 9, 0],
  );
  assert.deepEqual(quantizePixels(data, 10), ["#FF0000", "#0000FF", "#00FF00"]);
});

test("quantizePixels merges near-identical shades and honours the count", () => {
  const data = pixels(
    [50, 200, 10, 10, 255],
    [40, 215, 20, 20, 255],
    [20, 0, 0, 0, 255],
  );
  assert.equal(quantizePixels(data, 10).length, 2);
  assert.equal(quantizePixels(data, 1).length, 1);
  assert.deepEqual(quantizePixels(new Uint8Array(0), 5), []);
});

test("audioCodecArgs uses the native Opus encoder with its required flags", () => {
  assert.deepEqual(audioCodecArgs("aac"), ["-c:a", "aac"]);
  assert.deepEqual(audioCodecArgs("opus"), [
    "-c:a",
    "opus",
    "-strict",
    "-2",
    "-ar",
    "48000",
    "-ac",
    "2",
  ]);
});

test("codec tables only list encoders that work in the single-threaded core", () => {
  const broken = ["libx265", "libvpx-vp9", "libxvid"];
  for (const codecs of Object.values(VIDEO_FORMAT_CODEC_MAP)) {
    for (const codec of codecs) assert.ok(!broken.includes(codec), codec);
  }
  assert.deepEqual(videoContainerCodecArgs("webm"), [
    "-c:v",
    "libvpx",
    "-c:a",
    "libvorbis",
  ]);
});

test("describeFFmpegFailure surfaces the real reason, not the generic trailer", () => {
  assert.equal(
    describeFFmpegFailure([
      "Stream mapping:",
      "[mp4 @ 0x1] Could not find tag for codec vorbis in stream #1",
      "Conversion failed!",
      "Aborted()",
    ]),
    "Processing failed: Could not find tag for codec vorbis in stream #1",
  );
  assert.match(describeFFmpegFailure([]), /not supported/);
});

// The native app (app/) reads tools.json; the website switches to it in M7. Until then
// both registries exist, and this keeps them identical.
test("tools.json matches the registry in src/config/tools.ts", () => {
  const json = JSON.parse(
    readFileSync(new URL("../../tools.json", import.meta.url), "utf8"),
  );
  assert.deepEqual(
    json.categories,
    toolCategories.map(({ name, accentColor }) => ({
      name,
      accent: accentColor,
    })),
  );
  assert.deepEqual(
    json.tools.map(
      ({
        slug,
        name,
        shortName,
        description,
        category,
      }: Record<string, string>) => ({
        slug,
        name,
        shortName,
        description,
        category,
      }),
    ),
    tools.map(({ slug, name, shortName, description, category }) => ({
      slug,
      name,
      shortName,
      description,
      category,
    })),
  );
});

test("parseRanges handles lists, ranges, duplicates and out-of-bounds input", () => {
  assert.deepEqual(parseRanges("1-3, 5", 6), [0, 1, 2, 4]);
  assert.deepEqual(parseRanges("3, 1-2, 2", 3), [0, 1, 2]);
  assert.deepEqual(parseRanges("0, 9, 4-2, 2-99", 5), []);
  assert.deepEqual(parseRanges("  ", 5), []);
});
