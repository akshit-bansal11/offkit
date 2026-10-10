export interface ExtractedPaletteResponse {
  colors: string[];
}

const SAMPLE_SIZE = 96;
const BUCKET_BITS = 4;
const MIN_ALPHA = 128;
// Squared RGB distance below which two colours read as the same swatch.
const MIN_DISTANCE_SQ = 40 * 40;

type Rgb = [number, number, number];

/**
 * Pick the dominant colours from raw RGBA pixels: histogram into coarse RGB
 * buckets, average each bucket, then take the most populated buckets that are
 * visibly distinct from the ones already chosen.
 */
export function quantizePixels(
  data: Uint8ClampedArray | Uint8Array,
  count: number,
): string[] {
  const buckets = new Map<number, { n: number; r: number; g: number; b: number }>();
  const shift = 8 - BUCKET_BITS;

  for (let i = 0; i + 3 < data.length; i += 4) {
    if (data[i + 3] < MIN_ALPHA) continue;

    const key =
      ((data[i] >> shift) << (BUCKET_BITS * 2)) |
      ((data[i + 1] >> shift) << BUCKET_BITS) |
      (data[i + 2] >> shift);
    const bucket = buckets.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
    bucket.n += 1;
    bucket.r += data[i];
    bucket.g += data[i + 1];
    bucket.b += data[i + 2];
    buckets.set(key, bucket);
  }

  const ranked = [...buckets.values()]
    .sort((a, b) => b.n - a.n)
    .map(
      (bucket): Rgb => [
        Math.round(bucket.r / bucket.n),
        Math.round(bucket.g / bucket.n),
        Math.round(bucket.b / bucket.n),
      ],
    );

  const picked: Rgb[] = [];
  for (const candidate of ranked) {
    if (picked.length >= count) break;
    const distinct = picked.every(
      (color) =>
        (color[0] - candidate[0]) ** 2 +
          (color[1] - candidate[1]) ** 2 +
          (color[2] - candidate[2]) ** 2 >=
        MIN_DISTANCE_SQ,
    );
    if (distinct) picked.push(candidate);
  }

  return picked.map(
    (color) =>
      `#${color.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`.toUpperCase(),
  );
}

/** Extract up to `count` dominant colours from an image, entirely on-device. */
export async function extractPalette(
  file: Blob,
  count = 10,
): Promise<ExtractedPaletteResponse> {
  // createImageBitmap, not HTMLImageElement.decode(): the latter never
  // resolves while the tab is in the background.
  const image = await createImageBitmap(file);

  const scale = Math.min(1, SAMPLE_SIZE / Math.max(image.width, image.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(image.width * scale));
  canvas.height = Math.max(1, Math.round(image.height * scale));

  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    throw new Error("This browser could not read the image pixels.");
  }

  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  image.close();
  const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
  const colors = quantizePixels(data, count);

  if (colors.length === 0) {
    throw new Error("The image has no visible pixels to sample.");
  }

  return { colors };
}
