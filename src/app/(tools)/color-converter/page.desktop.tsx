"use client";

import { Droplets, Pipette } from "lucide-react";
import { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { OutputField } from "@/components/ui/design/OutputField";
import { Badge } from "@/components/ui/feedback/Badge";
import { Input } from "@/components/ui/form/Input";
import { Card, CardHeader, CardTitle } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import {
  formatColorOutputs,
  parseColorInput,
  rgbaToCssColor,
} from "@/lib/design-tools/colors";

const tool = getToolBySlug("color-converter");

export default function ColorConverterPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ColorConverterTool />
    </ToolPageShell>
  );
}

// A hex value, not a named colour: names are resolved through the DOM, so
// the server could not parse the sample and hydration mismatched.
const SAMPLE_COLOR = "#663399";

// ─── Tailwind color palette (v3) ─────────────────────────────────────────────
// hex → tailwind class name
const TW_PALETTE: Record<string, string> = {
  "#f8fafc": "slate-50",
  "#f1f5f9": "slate-100",
  "#e2e8f0": "slate-200",
  "#cbd5e1": "slate-300",
  "#94a3b8": "slate-400",
  "#64748b": "slate-500",
  "#475569": "slate-600",
  "#334155": "slate-700",
  "#1e293b": "slate-800",
  "#0f172a": "slate-900",
  "#020617": "slate-950",
  "#f9fafb": "gray-50",
  "#f3f4f6": "gray-100",
  "#e5e7eb": "gray-200",
  "#d1d5db": "gray-300",
  "#9ca3af": "gray-400",
  "#6b7280": "gray-500",
  "#4b5563": "gray-600",
  "#374151": "gray-700",
  "#1f2937": "gray-800",
  "#111827": "gray-900",
  "#030712": "gray-950",
  "#fafafa": "zinc-50",
  "#f4f4f5": "zinc-100",
  "#e4e4e7": "zinc-200",
  "#d4d4d8": "zinc-300",
  "#a1a1aa": "zinc-400",
  "#71717a": "zinc-500",
  "#52525b": "zinc-600",
  "#3f3f46": "zinc-700",
  "#27272a": "zinc-800",
  "#18181b": "zinc-900",
  "#09090b": "zinc-950",
  "#fef2f2": "red-50",
  "#fee2e2": "red-100",
  "#fecaca": "red-200",
  "#fca5a5": "red-300",
  "#f87171": "red-400",
  "#ef4444": "red-500",
  "#dc2626": "red-600",
  "#b91c1c": "red-700",
  "#991b1b": "red-800",
  "#7f1d1d": "red-900",
  "#450a0a": "red-950",
  "#fff7ed": "orange-50",
  "#ffedd5": "orange-100",
  "#fed7aa": "orange-200",
  "#fdba74": "orange-300",
  "#fb923c": "orange-400",
  "#f97316": "orange-500",
  "#ea580c": "orange-600",
  "#c2410c": "orange-700",
  "#9a3412": "orange-800",
  "#7c2d12": "orange-900",
  "#431407": "orange-950",
  "#fefce8": "yellow-50",
  "#fef9c3": "yellow-100",
  "#fef08a": "yellow-200",
  "#fde047": "yellow-300",
  "#facc15": "yellow-400",
  "#eab308": "yellow-500",
  "#ca8a04": "yellow-600",
  "#a16207": "yellow-700",
  "#854d0e": "yellow-800",
  "#713f12": "yellow-900",
  "#422006": "yellow-950",
  "#f0fdf4": "green-50",
  "#dcfce7": "green-100",
  "#bbf7d0": "green-200",
  "#86efac": "green-300",
  "#4ade80": "green-400",
  "#22c55e": "green-500",
  "#16a34a": "green-600",
  "#15803d": "green-700",
  "#166534": "green-800",
  "#14532d": "green-900",
  "#052e16": "green-950",
  "#eff6ff": "blue-50",
  "#dbeafe": "blue-100",
  "#bfdbfe": "blue-200",
  "#93c5fd": "blue-300",
  "#60a5fa": "blue-400",
  "#3b82f6": "blue-500",
  "#2563eb": "blue-600",
  "#1d4ed8": "blue-700",
  "#1e40af": "blue-800",
  "#1e3a8a": "blue-900",
  "#172554": "blue-950",
  "#f5f3ff": "violet-50",
  "#ede9fe": "violet-100",
  "#ddd6fe": "violet-200",
  "#c4b5fd": "violet-300",
  "#a78bfa": "violet-400",
  "#8b5cf6": "violet-500",
  "#7c3aed": "violet-600",
  "#6d28d9": "violet-700",
  "#5b21b6": "violet-800",
  "#4c1d95": "violet-900",
  "#2e1065": "violet-950",
  "#fdf4ff": "fuchsia-50",
  "#fae8ff": "fuchsia-100",
  "#f5d0fe": "fuchsia-200",
  "#f0abfc": "fuchsia-300",
  "#e879f9": "fuchsia-400",
  "#d946ef": "fuchsia-500",
  "#c026d3": "fuchsia-600",
  "#a21caf": "fuchsia-700",
  "#86198f": "fuchsia-800",
  "#701a75": "fuchsia-900",
  "#4a044e": "fuchsia-950",
  "#fdf2f8": "pink-50",
  "#fce7f3": "pink-100",
  "#fbcfe8": "pink-200",
  "#f9a8d4": "pink-300",
  "#f472b6": "pink-400",
  "#ec4899": "pink-500",
  "#db2777": "pink-600",
  "#be185d": "pink-700",
  "#9d174d": "pink-800",
  "#831843": "pink-900",
  "#500724": "pink-950",
  "#fff": "white",
  "#ffffff": "white",
  "#000": "black",
  "#000000": "black",
};

// ─── Format detection ─────────────────────────────────────────────────────────
type ColorFormat = "RGBA" | "HEXA" | "HSLA" | "Tailwind";

function detectFormat(value: string): ColorFormat {
  const v = value.trim().toLowerCase();
  if (v.startsWith("rgba(") || (v.startsWith("rgb(") && !v.startsWith("rgba(")))
    return "RGBA";
  if (v.startsWith("hsla(") || v.startsWith("hsl(")) return "HSLA";
  if (v.startsWith("#")) return "HEXA";
  // Tailwind: bg-{color}-{shade} or text-{color}-{shade} or just color-shade
  if (
    /^(bg|text|border|ring|fill|stroke)-[a-z]+-\d{2,3}$/.test(v) ||
    /^[a-z]+-\d{2,3}$/.test(v)
  )
    return "Tailwind";
  return "HEXA"; // default for named colors
}

// ─── Clean Tailwind input to a parseable hex ──────────────────────────────────
function cleanTailwindInput(value: string): string {
  const v = value
    .trim()
    .toLowerCase()
    .replace(/^(bg|text|border|ring|fill|stroke)-/, "");
  // Find matching hex in palette
  const entry = Object.entries(TW_PALETTE).find(([, name]) => name === v);
  return entry ? entry[0] : value;
}

// ─── Tailwind output ──────────────────────────────────────────────────────────
function toTailwindClass(hex: string): string {
  const normalized = hex.toLowerCase();
  const exact = TW_PALETTE[normalized];
  if (exact) return `bg-${exact}`;

  // Find nearest by hex distance
  let best = "";
  let bestDist = Infinity;
  const r1 = parseInt(normalized.slice(1, 3), 16);
  const g1 = parseInt(normalized.slice(3, 5), 16);
  const b1 = parseInt(normalized.slice(5, 7), 16);
  for (const [h, name] of Object.entries(TW_PALETTE)) {
    if (h.length < 7) continue;
    const r2 = parseInt(h.slice(1, 3), 16);
    const g2 = parseInt(h.slice(3, 5), 16);
    const b2 = parseInt(h.slice(5, 7), 16);
    const dist = Math.sqrt((r1 - r2) ** 2 + (g1 - g2) ** 2 + (b1 - b2) ** 2);
    if (dist < bestDist) {
      bestDist = dist;
      best = name;
    }
  }
  return best
    ? `bg-${best} /* nearest match, Δ${Math.round(bestDist)} */`
    : `bg-[${hex}]`;
}

// ─── Output pairs by detected format ─────────────────────────────────────────
function getOutputPairs(
  fmt: ColorFormat,
  outputs: ReturnType<typeof formatColorOutputs>,
  twClass: string,
) {
  const all = {
    RGBA: {
      label: "RGBA",
      description: "Red, green, blue, alpha.",
      value: outputs.rgb,
    },
    HEXA: {
      label: "HEX / HEXA",
      description: "Hex with optional alpha channel.",
      value: outputs.hex,
    },
    HSLA: {
      label: "HSLA",
      description: "Hue, saturation, lightness, alpha.",
      value: outputs.hsl,
    },
    Tailwind: {
      label: "Tailwind",
      description: "Nearest Tailwind utility class.",
      value: twClass,
    },
  };
  return (Object.keys(all) as ColorFormat[])
    .filter((k) => k !== fmt)
    .map((k) => all[k]);
}

// ─── Component ────────────────────────────────────────────────────────────────
function ColorConverterTool() {
  const [input, setInput] = useState(SAMPLE_COLOR);

  const detectedFormat = detectFormat(input);

  const cleanedInput = useMemo(() => {
    if (detectedFormat === "Tailwind") return cleanTailwindInput(input);
    return input;
  }, [input, detectedFormat]);

  const parsed = parseColorInput(cleanedInput);
  const outputs = parsed ? formatColorOutputs(parsed) : null;
  const hasError = input.trim() !== "" && !parsed;

  const twClass = outputs
    ? toTailwindClass(outputs.hex.split("/")[0].trim().toLowerCase())
    : "";

  const outputPairs = outputs
    ? getOutputPairs(detectedFormat, outputs, twClass)
    : [];

  return (
    <div className="space-y-4">
      {/* Input */}
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="space-y-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <CardTitle>Color Converter</CardTitle>
              <p className="mt-1.5 text-sm text-muted-foreground">
                Enter any color — HEX/A, RGB/A, HSL/A, or a Tailwind class. The
                other three formats are generated automatically.
              </p>
            </div>
            <Badge
              variant="secondary"
              className="border-primary/50 text-primary uppercase shrink-0"
            >
              Detected: {detectedFormat}
            </Badge>
          </div>

          <Input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            spellCheck={false}
            className="font-mono h-11"
            placeholder="e.g. #7C3AED, rgba(124,58,237,1), hsl(263 83% 58%), violet-600"
          />

          {hasError && (
            <div className="rounded-xl border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-300">
              Cannot parse color. Try a valid HEX, RGB/A, HSL/A, or Tailwind
              shade (e.g. <code>violet-600</code>).
            </div>
          )}
        </CardHeader>
      </Card>

      {/* Preview */}
      {parsed ? (
        <Card className="border-white/10 bg-card/70 overflow-hidden">
          <div className="flex items-center justify-between px-6 py-3 border-b border-white/5">
            <CardTitle className="text-sm font-medium">Live Preview</CardTitle>
            <span className="text-xs text-muted-foreground font-mono flex items-center gap-1.5">
              <Pipette size={11} />
              {rgbaToCssColor(parsed)}
            </span>
          </div>
          <div
            className="h-40 w-full transition-all duration-500"
            style={{ background: rgbaToCssColor(parsed) }}
          />
        </Card>
      ) : (
        <Card className="flex items-center justify-center h-40 border-dashed border-white/10 bg-card/30">
          <div className="flex items-center gap-2 text-muted-foreground/40">
            <Droplets size={15} />
            <span className="text-sm">Awaiting valid color…</span>
          </div>
        </Card>
      )}

      {/* Outputs — 3 columns */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        {outputPairs.map((o) => (
          <OutputField
            key={o.label}
            label={o.label}
            description={o.description}
            value={o.value}
          />
        ))}
      </div>
    </div>
  );
}
