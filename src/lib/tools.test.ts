// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

import { siteConfig } from "../config/site.ts";
import {
  getToolMetadata,
  runsOnWeb,
  toolCategories,
  tools,
} from "../config/tools.ts";
import { quantizePixels } from "./design-tools/palette-extractor.ts";
import {
  audioCodecArgs,
  describeFFmpegFailure,
  VIDEO_FORMAT_CODEC_MAP,
  videoContainerCodecArgs,
} from "./ffmpeg/client.ts";

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

// The native app (desktop/) reads tools.json; the website switches to it in M7. Until then
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
  const pick = ({
    slug,
    platforms,
    name,
    shortName,
    description,
    category,
  }: {
    slug: string;
    platforms: string[];
    name: string;
    shortName?: string;
    description: string;
    category: string;
  }) => ({ slug, platforms, name, shortName, description, category });
  assert.deepEqual(json.tools.map(pick), tools.map(pick));
  assert.equal(json.siteUrl, siteConfig.url);
});

test("runsOnWeb is false for a tool that lists only the desktop", () => {
  assert.equal(runsOnWeb({ platforms: ["web", "desktop"] }), true);
  assert.equal(runsOnWeb({ platforms: ["web"] }), true);
  assert.equal(runsOnWeb({ platforms: ["desktop"] }), false);
  assert.ok(tools.every((tool) => tool.platforms.length > 0));
});

test("every registered tool has a page, and every tool page is registered", () => {
  const toolsDir = new URL("../app/(tools)/", import.meta.url);
  const pageSlugs = readdirSync(toolsDir).filter((entry) =>
    existsSync(new URL(`${entry}/page.tsx`, toolsDir)),
  );

  assert.deepEqual(pageSlugs.sort(), tools.map((tool) => tool.slug).sort());
});

test("getToolMetadata gives a tool page its own title, description and canonical path", () => {
  const metadata = getToolMetadata("json-formatter");

  assert.deepEqual(metadata, {
    title: "JSON Formatter",
    description:
      "Pretty-print, minify, validate, and copy JSON on your device.",
    alternates: { canonical: "/json-formatter" },
    openGraph: {
      title: "JSON Formatter",
      description:
        "Pretty-print, minify, validate, and copy JSON on your device.",
      url: "/json-formatter",
      images: [
        {
          url: "/opengraph-image",
          width: 1200,
          height: 630,
          alt: "Offkit: file tools that run on your device",
        },
      ],
    },
  });
});

test("getToolMetadata rejects a slug that is not registered", () => {
  assert.throws(() => getToolMetadata("no-such-tool"), /no-such-tool/);
});
