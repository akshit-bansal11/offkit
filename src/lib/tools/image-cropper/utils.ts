import { MIN_CROP_SIZE, type AspectValue, ASPECT_PRESETS } from "./constants";
import type { CropRect, Handle, UploadedImage } from "./types";

export function createDefaultCrop(width: number, height: number): CropRect {
  const targetW = Math.max(MIN_CROP_SIZE, Math.round(width * 0.7));
  const targetH = Math.max(MIN_CROP_SIZE, Math.round(height * 0.7));
  const x = Math.round((width - targetW) / 2);
  const y = Math.round((height - targetH) / 2);
  return { x, y, w: targetW, h: targetH };
}

export function clampRect(rect: CropRect, width: number, height: number): CropRect {
  const w = Math.max(MIN_CROP_SIZE, Math.min(rect.w, width));
  const h = Math.max(MIN_CROP_SIZE, Math.min(rect.h, height));
  const x = Math.max(0, Math.min(rect.x, width - w));
  const y = Math.max(0, Math.min(rect.y, height - h));
  return { x, y, w, h };
}

export async function loadImage(url: string): Promise<HTMLImageElement> {
  const image = new Image();
  image.decoding = "async";
  image.src = url;

  await new Promise<void>((resolve, reject) => {
    image.onload = () => resolve();
    image.onerror = () => reject(new Error("Unable to load image."));
  });

  return image;
}

export async function cropToBlob(image: UploadedImage, crop: CropRect): Promise<Blob> {
  const source = await loadImage(image.srcUrl);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(crop.w));
  canvas.height = Math.max(1, Math.round(crop.h));
  const context = canvas.getContext("2d");

  if (!context) throw new Error("Unable to initialize image canvas context.");

  context.drawImage(
    source,
    crop.x,
    crop.y,
    crop.w,
    crop.h,
    0,
    0,
    crop.w,
    crop.h,
  );

  const blob = await new Promise<Blob | null>((resolve) => {
    canvas.toBlob(resolve, "image/png", 0.95);
  });

  if (!blob) throw new Error("Failed to export cropped image.");
  return blob;
}

export function clampBatchCropToImage(batchCrop: CropRect, image: UploadedImage): CropRect {
  return clampRect(batchCrop, image.width, image.height);
}

export function getAspectRatio(value: AspectValue): number | null {
  return ASPECT_PRESETS.find((preset) => preset.value === value)?.ratio ?? null;
}

export function createDefaultCircularCrop(width: number, height: number): CropRect {
  const minDim = Math.min(width, height);
  const radius = Math.round(minDim * 0.3);
  const centerX = Math.round(width / 2);
  const centerY = Math.round(height / 2);
  return {
    x: Math.round(centerX - radius),
    y: Math.round(centerY - radius),
    w: radius * 2,
    h: radius * 2,
  };
}

/**
 * Convert a pointer position to original-image pixels. `scale` is the factor
 * the canvas was drawn at; crop rectangles are stored in image pixels, so
 * comparing them against raw canvas coordinates is wrong for scaled images.
 */
export function pointToImageSpace(
  canvas: HTMLCanvasElement,
  clientX: number,
  clientY: number,
  scale: number,
) {
  const rect = canvas.getBoundingClientRect();
  return {
    x: ((clientX - rect.left) * canvas.width) / rect.width / scale,
    y: ((clientY - rect.top) * canvas.height) / rect.height / scale,
  };
}

/** Largest rectangle of the given ratio centred inside `crop`, kept in bounds. */
export function fitCropToAspect(
  crop: CropRect,
  ratio: number,
  width: number,
  height: number,
): CropRect {
  const w = Math.min(crop.w, crop.h * ratio);
  const h = w / ratio;
  return clampRect(
    {
      x: Math.round(crop.x + (crop.w - w) / 2),
      y: Math.round(crop.y + (crop.h - h) / 2),
      w: Math.round(w),
      h: Math.round(h),
    },
    width,
    height,
  );
}

export function getHandleAtPoint(
  point: { x: number; y: number },
  crop: CropRect,
  tolerance: number,
): Handle | null {
  const points: Array<{ handle: Handle; x: number; y: number }> = [
    { handle: "nw", x: crop.x, y: crop.y },
    { handle: "n", x: crop.x + crop.w / 2, y: crop.y },
    { handle: "ne", x: crop.x + crop.w, y: crop.y },
    { handle: "e", x: crop.x + crop.w, y: crop.y + crop.h / 2 },
    { handle: "se", x: crop.x + crop.w, y: crop.y + crop.h },
    { handle: "s", x: crop.x + crop.w / 2, y: crop.y + crop.h },
    { handle: "sw", x: crop.x, y: crop.y + crop.h },
    { handle: "w", x: crop.x, y: crop.y + crop.h / 2 },
  ];

  for (const candidate of points) {
    if (
      Math.abs(point.x - candidate.x) <= tolerance &&
      Math.abs(point.y - candidate.y) <= tolerance
    ) {
      return candidate.handle;
    }
  }

  return point.x >= crop.x &&
    point.x <= crop.x + crop.w &&
    point.y >= crop.y &&
    point.y <= crop.y + crop.h
    ? "move"
    : null;
}

export function resizeRect(
  start: CropRect,
  dx: number,
  dy: number,
  handle: Handle,
  ratio: number | null,
): CropRect {
  const next: CropRect = { ...start };
  const right = start.x + start.w;
  const bottom = start.y + start.h;

  if (handle === "move") {
    return { ...start, x: start.x + dx, y: start.y + dy };
  }

  if (handle.includes("e")) next.w = start.w + dx;
  if (handle.includes("w")) {
    next.x = start.x + dx;
    next.w = right - next.x;
  }
  if (handle.includes("s")) next.h = start.h + dy;
  if (handle.includes("n")) {
    next.y = start.y + dy;
    next.h = bottom - next.y;
  }

  if (!ratio) return next;

  const horizontal = handle.includes("e") || handle.includes("w");
  const vertical = handle.includes("n") || handle.includes("s");

  if (horizontal && !vertical) {
    next.h = next.w / ratio;
    next.y = start.y + (start.h - next.h) / 2;
    return next;
  }

  if (vertical && !horizontal) {
    next.w = next.h * ratio;
    next.x = start.x + (start.w - next.w) / 2;
    return next;
  }

  if (Math.abs(dx) >= Math.abs(dy)) {
    next.h = next.w / ratio;
  } else {
    next.w = next.h * ratio;
  }

  if (handle.includes("w")) next.x = right - next.w;
  if (handle.includes("n")) next.y = bottom - next.h;
  return next;
}

export function getCenterAndRadiusFromBounds(bounds: CropRect): { cx: number; cy: number; r: number } {
  const cx = bounds.x + bounds.w / 2;
  const cy = bounds.y + bounds.h / 2;
  const r = bounds.w / 2;
  return { cx, cy, r };
}

export async function cropToCircularBlob(image: UploadedImage, crop: CropRect): Promise<Blob> {
  const source = await loadImage(image.srcUrl);
  const { r } = getCenterAndRadiusFromBounds(crop);

  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(crop.w));
  canvas.height = Math.max(1, Math.round(crop.h));

  const context = canvas.getContext("2d");
  if (!context) throw new Error("Unable to initialize image canvas context.");

  // Create circular clipping region
  context.beginPath();
  context.arc(r, r, r, 0, Math.PI * 2);
  context.clip();

  // Draw the cropped region
  context.drawImage(
    source,
    crop.x,
    crop.y,
    crop.w,
    crop.h,
    0,
    0,
    crop.w,
    crop.h,
  );

  const blob = await new Promise<Blob | null>((resolve) => {
    canvas.toBlob(resolve, "image/png", 0.95);
  });

  if (!blob) throw new Error("Failed to export cropped image.");
  return blob;
}


