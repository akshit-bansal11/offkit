import {
  FORMAT_LABELS,
  FORMAT_MIME_MAP,
  type ImageFormat,
  LOSSY_FORMATS,
  SUPPORTED_OUTPUT_FORMATS,
} from "./converter";

type MagickModule = typeof import("@imagemagick/magick-wasm");
const ICO_MAX_DIMENSION = 512;

const MAGICK_FORMAT_MAP: Record<ImageFormat, string> = {
  png: "PNG",
  jpg: "JPG",
  jpeg: "JPEG",
  webp: "WEBP",
  avif: "AVIF",
  tiff: "TIFF",
  heif: "HEIF",
  ico: "ICO",
};

let magickModulePromise: Promise<MagickModule> | null = null;

function fitWithinLimit(width: number, height: number, limit: number) {
  const maxDimension = Math.max(width, height);
  if (maxDimension <= limit) {
    return null;
  }

  const scale = limit / maxDimension;
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

async function getMagickModule(): Promise<MagickModule> {
  if (!magickModulePromise) {
    magickModulePromise = (async () => {
      const magickModule = await import("@imagemagick/magick-wasm");
      const response = await fetch("/api/magick-wasm");

      if (!response.ok) {
        throw new Error("Failed to load the ImageMagick runtime.");
      }

      const wasmBytes = new Uint8Array(await response.arrayBuffer());
      await magickModule.initializeImageMagick(wasmBytes);
      return magickModule;
    })();
  }

  return magickModulePromise;
}

export async function getImageMetadata(file: File): Promise<{
  width: number;
  height: number;
}> {
  const magickModule = await getMagickModule();
  const sourceBytes = new Uint8Array(await file.arrayBuffer());
  const info = magickModule.MagickImageInfo.create(sourceBytes);

  return {
    width: info.width,
    height: info.height,
  };
}

/**
 * Convert an image file to another format using the bundled ImageMagick WASM codec.
 */
export async function convertImage(
  file: File,
  targetFormat: ImageFormat,
  quality: number,
): Promise<Blob> {
  if (!SUPPORTED_OUTPUT_FORMATS.includes(targetFormat)) {
    throw new Error(
      `${FORMAT_LABELS[targetFormat]} export is not available in this build yet.`,
    );
  }

  const magickModule = await getMagickModule();
  const sourceBytes = new Uint8Array(await file.arrayBuffer());

  try {
    return magickModule.ImageMagick.read(sourceBytes, async (image) => {
      if (LOSSY_FORMATS.includes(targetFormat)) {
        image.quality = quality;
      }

      if (targetFormat === "ico") {
        const resized = fitWithinLimit(
          image.width,
          image.height,
          ICO_MAX_DIMENSION,
        );

        if (resized) {
          image.resize(resized.width, resized.height);
        }
      }

      if (targetFormat === "jpg" || targetFormat === "jpeg") {
        image.backgroundColor = new magickModule.MagickColor("#FFFFFF");
        if (image.hasAlpha) {
          image.alpha(magickModule.AlphaAction.Remove);
        }
      }

      return image.write(
        MAGICK_FORMAT_MAP[
          targetFormat
        ] as import("@imagemagick/magick-wasm").MagickFormat,
        (data) =>
          new Blob([Uint8Array.from(data)], {
            type: FORMAT_MIME_MAP[targetFormat],
          }),
      );
    });
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Unknown conversion error";
    throw new Error(
      `Conversion to ${FORMAT_LABELS[targetFormat]} failed. ${message}`,
    );
  }
}
