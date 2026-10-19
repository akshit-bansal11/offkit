"use client";

import { ArrowRightLeft, Wand2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { OutputField } from "@/components/ui/design/OutputField";
import { Badge } from "@/components/ui/feedback/Badge";
import { Textarea } from "@/components/ui/form/Textarea";
import { Card, CardHeader, CardTitle } from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { getToolBySlug } from "@/config/tools";
import {
  convertGradientSyntax,
  type GradientKind,
  normalizeGradientInput,
  type ParsedGradient,
  parseGradientString,
} from "@/lib/design-tools/gradients";

const tool = getToolBySlug("gradient-converter");

export default function GradientConverterPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <GradientConverterTool />
    </ToolPageShell>
  );
}

const SAMPLE_GRADIENT =
  "linear-gradient(135deg, #2563EB 0%, #7C3AED 50%, #EC4899 100%)";

function detectFormat(value: string): "Tailwind" | "Bootstrap" | "CSS" {
  if (value.includes("bg-[") || value.match(/\bbg-gradient-to-/))
    return "Tailwind";
  if (value.includes("var(--bs-gradient)") || value.includes(".bg-gradient"))
    return "Bootstrap";
  return "CSS";
}

function getTailwindClasses(parsedGradient: ParsedGradient): string {
  if (parsedGradient.type !== "linear" || parsedGradient.stops.length > 3) {
    return `bg-[${parsedGradient.normalized.replace(/\s+/g, "_")}]`;
  }

  const dirMap: Record<string, string> = {
    "to top": "t",
    "0deg": "t",
    "to top right": "tr",
    "45deg": "tr",
    "to right": "r",
    "90deg": "r",
    "to bottom right": "br",
    "135deg": "br",
    "to bottom": "b",
    "180deg": "b",
    "to bottom left": "bl",
    "225deg": "bl",
    "to left": "l",
    "270deg": "l",
    "to top left": "tl",
    "315deg": "tl",
  };

  const dir = dirMap[parsedGradient.orientation.trim().toLowerCase()];
  if (!dir) return `bg-[${parsedGradient.normalized.replace(/\s+/g, "_")}]`;

  const colors = parsedGradient.stops.map(
    (s: string) => s.trim().split(/\s+/)[0],
  );
  let cls = `bg-gradient-to-${dir} from-[${colors[0]}]`;
  if (colors.length === 3) cls += ` via-[${colors[1]}] to-[${colors[2]}]`;
  else if (colors.length === 2) cls += ` to-[${colors[1]}]`;
  return cls;
}

function GradientConverterTool() {
  const [input, setInput] = useState(SAMPLE_GRADIENT);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [input]);

  const cleanInput = useMemo(() => {
    let c = input.trim().replace(/;$/, "");

    // Tailwind utility: bg-gradient-to-{dir} from-[...] via-[...] to-[...]
    const twUtility = c.match(
      /\bbg-gradient-to-([a-z]+)\s+from-\[([^\]]+)\](?:\s+via-\[([^\]]+)\])?(?:\s+to-\[([^\]]+)\])?/,
    );
    if (twUtility) {
      const dirMap: Record<string, string> = {
        t: "to top",
        tr: "to top right",
        r: "to right",
        br: "to bottom right",
        b: "to bottom",
        bl: "to bottom left",
        l: "to left",
        tl: "to top left",
      };
      const dir = dirMap[twUtility[1]] ?? `to ${twUtility[1]}`;
      const stops = [twUtility[2], twUtility[3], twUtility[4]].filter(Boolean);
      return `linear-gradient(${dir}, ${stops.join(", ")})`;
    }

    // Tailwind arbitrary: bg-[linear-gradient(...)]
    if (c.startsWith("bg-[") && c.endsWith("]")) {
      c = c.slice(4, -1).replace(/_/g, " ");
    }

    // CSS property prefix
    c = c.replace(/^(background-image|background|--bs-gradient)\s*:\s*/i, "");

    // Bootstrap class block — extract the background-image value
    const bsMatch = c.match(/background-image\s*:\s*([^;]+)/i);
    if (bsMatch) c = bsMatch[1].trim();

    return c;
  }, [input]);

  const parsed = parseGradientString(normalizeGradientInput(cleanInput));
  const detectedFormat = detectFormat(input);
  const hasError = input.trim() !== "" && !parsed;

  const cssOutput = parsed ? `background: ${parsed.normalized};` : "";
  const tailwindOutput = parsed ? getTailwindClasses(parsed) : "";
  const bootstrapOutput = parsed
    ? `.custom-gradient {\n  background-color: ${parsed.stops[0]?.split(" ")[0] ?? "#000"};\n  background-image: ${parsed.normalized};\n}`
    : "";

  // Show the two formats that are NOT the detected input
  const outputPairs: Record<
    typeof detectedFormat,
    { label: string; description: string; value: string }[]
  > = {
    CSS: [
      {
        label: "Tailwind CSS",
        description: "Utility classes or arbitrary value syntax.",
        value: tailwindOutput,
      },
      {
        label: "Bootstrap 5",
        description: "Custom utility using Bootstrap 5 background properties.",
        value: bootstrapOutput,
      },
    ],
    Tailwind: [
      {
        label: "Standard CSS",
        description: "Cross-browser CSS background property.",
        value: cssOutput,
      },
      {
        label: "Bootstrap 5",
        description: "Custom utility using Bootstrap 5 background properties.",
        value: bootstrapOutput,
      },
    ],
    Bootstrap: [
      {
        label: "Standard CSS",
        description: "Cross-browser CSS background property.",
        value: cssOutput,
      },
      {
        label: "Tailwind CSS",
        description: "Utility classes or arbitrary value syntax.",
        value: tailwindOutput,
      },
    ],
  };

  const outputs = outputPairs[detectedFormat];

  return (
    <div className="space-y-4">
      {/* Input */}
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="space-y-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <CardTitle>Gradient Converter</CardTitle>
              <p className="mt-1.5 text-sm text-muted-foreground">
                Paste any CSS, Tailwind, or Bootstrap gradient — outputs are
                generated automatically.
              </p>
            </div>
            <Badge
              variant="secondary"
              className="border-primary/50 text-primary uppercase shrink-0"
            >
              Detected: {detectedFormat}
            </Badge>
          </div>

          <div className="relative">
            <Textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              spellCheck={false}
              className="min-h-[52px] resize-none overflow-hidden font-mono text-sm leading-relaxed pr-9"
              placeholder="e.g. linear-gradient(135deg, #2563EB, #EC4899)"
              rows={1}
            />
            <Wand2
              size={14}
              className="absolute right-3 top-3 text-muted-foreground/40 pointer-events-none"
            />
          </div>

          {hasError && (
            <div className="rounded-xl border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive">
              Cannot parse gradient. Ensure it&apos;s a valid linear, radial, or
              conic gradient value.
            </div>
          )}
        </CardHeader>
      </Card>

      {/* Preview */}
      {parsed ? (
        <Card className="border-white/10 bg-card/70 overflow-hidden">
          <div className="flex items-center justify-between px-6 py-3 border-b border-white/5">
            <CardTitle className="text-sm font-medium">Live Preview</CardTitle>
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-xs text-muted-foreground font-mono">
                {parsed.stops.length} stops
              </span>
              <SegmentedControl
                variant="dark"
                size="sm"
                aria-label="Gradient type"
                value={parsed.type}
                onValueChange={(value) =>
                  setInput(convertGradientSyntax(parsed, value as GradientKind))
                }
                options={[
                  { label: "Linear", value: "linear" },
                  { label: "Radial", value: "radial" },
                  { label: "Conic", value: "conic" },
                ]}
              />
            </div>
          </div>
          <div
            className="h-40 w-full transition-all duration-500"
            style={{ backgroundImage: parsed.normalized }}
          />
        </Card>
      ) : (
        <Card className="flex items-center justify-center h-40 border-dashed border-white/10 bg-card/30">
          <div className="flex items-center gap-2 text-muted-foreground/40">
            <ArrowRightLeft size={15} />
            <span className="text-sm">Awaiting valid gradient…</span>
          </div>
        </Card>
      )}

      {/* Outputs — 2 columns */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {outputs.map((o) => (
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
