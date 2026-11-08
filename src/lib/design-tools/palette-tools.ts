// palette-tools.ts
// Pure helpers shared by Color Blocks and Palette Maker: hex validation, hue
// family, light/dark, random colours, reordering and palette exporters.

export const MIN_COLORS = 2;
export const MAX_COLORS = 10;

export const hueFamilies = [
  "red",
  "orange",
  "yellow",
  "green",
  "cyan",
  "blue",
  "purple",
  "pink",
  "neutral",
] as const;

export type HueFamily = (typeof hueFamilies)[number];
export type Lightness = "light" | "dark";

const HEX_PATTERN = /^#[0-9a-f]{6}$/i;

export function isValidHex(value: string): boolean {
  return HEX_PATTERN.test(value);
}

/** Lower-cases a valid #rrggbb value; returns null for anything else. */
export function normalizeHex(value: string): string | null {
  const trimmed = value.trim();
  return isValidHex(trimmed) ? trimmed.toLowerCase() : null;
}

function channels(hex: string): [number, number, number] {
  return [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
  ];
}

/** Hue in degrees (0-360), saturation and lightness in 0-1. */
function hexToHsl(hex: string): { h: number; s: number; l: number } {
  const [red, green, blue] = channels(hex);
  const r = red / 255;
  const g = green / 255;
  const b = blue / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const delta = max - min;

  if (delta === 0) {
    return { h: 0, s: 0, l };
  }

  const s = delta / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === r) {
    h = ((g - b) / delta) % 6;
  } else if (max === g) {
    h = (b - r) / delta + 2;
  } else {
    h = (r - g) / delta + 4;
  }

  return { h: (h * 60 + 360) % 360, s, l };
}

export function hueFamily(hex: string): HueFamily {
  const { h, s, l } = hexToHsl(hex);

  if (s < 0.12 || l < 0.08 || l > 0.95) {
    return "neutral";
  }
  if (h < 15 || h >= 350) return "red";
  if (h < 45) return "orange";
  if (h < 70) return "yellow";
  if (h < 165) return "green";
  if (h < 200) return "cyan";
  if (h < 255) return "blue";
  if (h < 290) return "purple";
  return "pink";
}

function linear(value: number): number {
  const v = value / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** Light when black text reads better on it than white text. */
export function lightness(hex: string): Lightness {
  const [r, g, b] = channels(hex);
  const luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
  return luminance > 0.179 ? "light" : "dark";
}

function hslToHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l);
  const channel = (n: number) => {
    const k = (n + h / 30) % 12;
    const value = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(value * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${channel(0)}${channel(8)}${channel(4)}`;
}

/** A random, usable colour: mid saturation and lightness. */
export function randomHex(random: () => number = Math.random): string {
  return hslToHex(
    Math.floor(random() * 360),
    0.45 + random() * 0.4,
    0.35 + random() * 0.4,
  );
}

/** Replaces every unlocked colour with a random one. */
export function randomizeColors(
  colors: readonly string[],
  locked: readonly boolean[],
  random: () => number = Math.random,
): string[] {
  return colors.map((color, index) =>
    locked[index] ? color : randomHex(random),
  );
}

/** Moves the item at `from` to `to`; an out-of-range move returns a copy. */
export function moveItem<T>(
  items: readonly T[],
  from: number,
  to: number,
): T[] {
  const next = [...items];
  const item = next[from];
  if (item === undefined || to < 0 || to >= next.length) {
    return next;
  }
  next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export function toCssVariables(colors: readonly string[]): string {
  const lines = colors.map((color, i) => `  --color-${i + 1}: ${color};`);
  return `:root {\n${lines.join("\n")}\n}`;
}

export function toTailwindTheme(colors: readonly string[]): string {
  const entries = Object.fromEntries(
    colors.map((color, i) => [`palette-${i + 1}`, color]),
  );
  return JSON.stringify({ theme: { extend: { colors: entries } } }, null, 2);
}

export function toJson(colors: readonly string[]): string {
  return JSON.stringify({ colors }, null, 2);
}
