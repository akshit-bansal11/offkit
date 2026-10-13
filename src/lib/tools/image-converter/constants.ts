import type { ImageFormat } from "@/lib/converters/image-converter/converter";

export const ACCEPTED_INPUT =
  ".png,.jpg,.jpeg,.webp,.avif,.tiff,.tif,.heif,.heic,.ico";

export const UNSUPPORTED_OUTPUT_FORMATS: ImageFormat[] = ["heif"];
export const UNSUPPORTED_OUTPUT_SET = new Set<ImageFormat>(
  UNSUPPORTED_OUTPUT_FORMATS,
);
