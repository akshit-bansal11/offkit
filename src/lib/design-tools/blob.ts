// blob.ts
// CSS and Tailwind output for the CSS Blob Generator, from one background value so the preview and both outputs agree.

// The gradient fades to the fill colour at 50% alpha, written as the hex suffix "80".
const GRADIENT_END_ALPHA = "80";

/** The blob's CSS background: a 135deg fade of the fill colour, or the flat colour. */
export function blobBackground(color: string, gradient: boolean): string {
  return gradient
    ? `linear-gradient(135deg, ${color}, ${color}${GRADIENT_END_ALPHA})`
    : color;
}

export function blobCss(
  radius: string,
  color: string,
  gradient: boolean,
): string {
  return [
    `border-radius: ${radius};`,
    `background: ${blobBackground(color, gradient)};`,
  ].join("\n");
}

/** Tailwind classes using arbitrary values, so any picked colour comes through exactly. */
export function blobTailwind(
  radius: string,
  color: string,
  gradient: boolean,
): string {
  const background = gradient
    ? `bg-[${blobBackground(color, true).replace(/\s+/g, "")}]`
    : `bg-[${color}]`;
  return `rounded-[${radius.replace(/ /g, "_")}] ${background}`;
}
