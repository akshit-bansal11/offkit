// compress.ts
// Rasterising PDF compression, shared by PDF Compressor and PDF Toolkit.

import { PDFDocument } from "pdf-lib";
import { pdfjsLib } from "@/lib/pdf/pdfjs";

interface CompressPdfOptions {
  /** pdf.js render scale: 1 is the page's own size in points. */
  scale: number;
  /** JPEG quality, 10 to 100. */
  quality: number;
  onProgress?: (fraction: number) => void;
}

export const NOT_SMALLER_MESSAGE =
  "A compressed copy would not be smaller than this PDF, so there is nothing to download. Try a lower render scale or JPEG quality.";

/**
 * Render every page to a JPEG and rebuild the PDF from those images. Text and
 * vectors become pixels, which shrinks scanned or image-heavy documents and
 * can grow text-only ones. Resolves null when the result is not smaller than
 * the input, so no caller can offer a file larger than the one it was given.
 */
export async function compressPdf(
  file: File,
  { scale, quality, onProgress }: CompressPdfOptions,
): Promise<Blob | null> {
  const source = await pdfjsLib.getDocument({ data: await file.arrayBuffer() })
    .promise;
  try {
    const output = await PDFDocument.create();

    for (let index = 1; index <= source.numPages; index += 1) {
      const page = await source.getPage(index);
      const viewport = page.getViewport({ scale });
      const canvas = document.createElement("canvas");
      const context = canvas.getContext("2d");
      if (!context) throw new Error("Canvas is not available in this browser.");
      canvas.width = viewport.width;
      canvas.height = viewport.height;
      await page.render({ canvas, canvasContext: context, viewport }).promise;

      const jpeg = await new Promise<Blob | null>((resolve) =>
        canvas.toBlob(resolve, "image/jpeg", quality / 100),
      );
      if (!jpeg) throw new Error(`Page ${index} could not be rendered.`);

      const image = await output.embedJpg(await jpeg.arrayBuffer());
      output.addPage([image.width, image.height]).drawImage(image, {
        x: 0,
        y: 0,
        width: image.width,
        height: image.height,
      });
      onProgress?.(index / source.numPages);
    }

    const bytes = await output.save();
    const blob = new Blob([Uint8Array.from(bytes)], {
      type: "application/pdf",
    });
    return blob.size < file.size ? blob : null;
  } finally {
    // Frees the worker-side copy of the document.
    await source.destroy();
  }
}
