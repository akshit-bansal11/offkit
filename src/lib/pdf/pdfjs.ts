// pdfjs.ts
// pdf.js with its worker pointed at the copy shipped in public/vendor.

import * as pdfjsLib from "pdfjs-dist/legacy/build/pdf.mjs";

if (typeof window !== "undefined") {
  // Copied from pdfjs-dist by scripts/copy-vendor.mjs.
  pdfjsLib.GlobalWorkerOptions.workerSrc = "/vendor/pdf.worker.min.mjs";
}

export { pdfjsLib };
