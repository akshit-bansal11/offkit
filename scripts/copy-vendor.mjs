// copy-vendor.mjs
// Copies the wasm engines and the pdf.js worker out of node_modules into public/vendor, so the app never fetches them from a CDN.
import { cpSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const files = [
  [
    "node_modules/@ffmpeg/core/dist/umd/ffmpeg-core.js",
    "public/vendor/ffmpeg/ffmpeg-core.js",
  ],
  [
    "node_modules/@ffmpeg/core/dist/umd/ffmpeg-core.wasm",
    "public/vendor/ffmpeg/ffmpeg-core.wasm",
  ],
  [
    // 0.0.44 ships dist/x86 (32-bit, the package's default "./magick.wasm" export) and dist/x64.
    "node_modules/@imagemagick/magick-wasm/dist/x86/magick.wasm",
    "public/vendor/magick.wasm",
  ],
  [
    "node_modules/pdfjs-dist/legacy/build/pdf.worker.min.mjs",
    "public/vendor/pdf.worker.min.mjs",
  ],
];

for (const [from, to] of files) {
  mkdirSync(dirname(root + to), { recursive: true });
  cpSync(root + from, root + to);
}
