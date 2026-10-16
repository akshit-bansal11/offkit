"use client";

import {
  Check,
  Copy as CopyIcon,
  Download,
  SlidersHorizontal,
} from "lucide-react";
import React, { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { ColorInput } from "@/components/ui/form/ColorInput";
import { Textarea } from "@/components/ui/form/Textarea";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("svg-pattern");

export default function SvgPatternPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <SvgPatternTool />
    </ToolPageShell>
  );
}

const EXPORT_WIDTH = 1600;
const EXPORT_HEIGHT = 900;

const PATTERNS = {
  grid: (fg: string, size: number, w: number) =>
    `<path d="M ${size} 0 L 0 0 0 ${size}" fill="none" stroke="${fg}" stroke-width="${w}"/>`,
  dots: (fg: string, size: number, w: number) =>
    `<circle cx="${size / 2}" cy="${size / 2}" r="${(size / 8) * w}" fill="${fg}"/>`,
  diagonal: (fg: string, size: number, w: number) =>
    `<path d="M0 ${size} L${size} 0" stroke="${fg}" stroke-width="${w}" fill="none"/>`,
  cross: (fg: string, size: number, w: number) =>
    `<path d="M${size / 2} 0v${size}M0 ${size / 2}h${size}" stroke="${fg}" stroke-width="${w}" fill="none"/>`,
  waves: (fg: string, size: number, w: number) =>
    `<path d="M0 ${size / 4} Q ${size / 4} 0 ${size / 2} ${size / 4} T ${size} ${size / 4}" stroke="${fg}" stroke-width="${w}" fill="none"/>`,
  zigzag: (fg: string, size: number, w: number) =>
    `<path d="M0 ${size / 2} L ${size / 4} 0 L ${size / 2} ${size / 2} L ${size * 0.75} 0 L ${size} ${size / 2}" stroke="${fg}" stroke-width="${w}" fill="none"/>`,
  rings: (fg: string, size: number, w: number) =>
    `<circle cx="${size / 2}" cy="${size / 2}" r="${size / 3}" stroke="${fg}" stroke-width="${w}" fill="none"/>`,
  checker: (fg: string, size: number) =>
    `<rect width="${size / 2}" height="${size / 2}" fill="${fg}"/><rect x="${size / 2}" y="${size / 2}" width="${size / 2}" height="${size / 2}" fill="${fg}"/>`,
  verticalLines: (fg: string, size: number, w: number) =>
    `<path d="M${size / 2} 0 V${size}" stroke="${fg}" stroke-width="${w}"/>`,
};

type PatternType = keyof typeof PATTERNS;
type SliderChangeEvent = React.ChangeEvent<HTMLInputElement>;

function getPatternHeight(pattern: PatternType, size: number) {
  return pattern === "waves" ? size / 2 : size;
}

function buildPatternMarkup(
  pattern: PatternType,
  fg: string,
  size: number,
  width: number,
) {
  const generator = PATTERNS[pattern];
  return generator(fg, size, width);
}

function buildTileSvg(
  pattern: PatternType,
  bgColor: string,
  fg: string,
  size: number,
  width: number,
) {
  const tileHeight = getPatternHeight(pattern, size);
  const markup = buildPatternMarkup(pattern, fg, size, width);

  return `<svg width="${size}" height="${tileHeight}" viewBox="0 0 ${size} ${tileHeight}" xmlns="http://www.w3.org/2000/svg"><rect width="${size}" height="${tileHeight}" fill="${bgColor}"/>${markup}</svg>`;
}

function buildExportSvg(
  pattern: PatternType,
  bgColor: string,
  fg: string,
  size: number,
  width: number,
) {
  const tileHeight = getPatternHeight(pattern, size);
  const markup = buildPatternMarkup(pattern, fg, size, width);

  return `<svg width="${EXPORT_WIDTH}" height="${EXPORT_HEIGHT}" viewBox="0 0 ${EXPORT_WIDTH} ${EXPORT_HEIGHT}" xmlns="http://www.w3.org/2000/svg"><defs><pattern id="pattern" patternUnits="userSpaceOnUse" width="${size}" height="${tileHeight}">${markup}</pattern></defs><rect width="${EXPORT_WIDTH}" height="${EXPORT_HEIGHT}" fill="${bgColor}"/><rect width="${EXPORT_WIDTH}" height="${EXPORT_HEIGHT}" fill="url(#pattern)"/></svg>`;
}

function SvgPatternTool() {
  const [activePattern, setActivePattern] = useState<PatternType>("dots");
  const [size, setSize] = useState(32);
  const [fgSize, setFgSize] = useState(1);
  const [bgColor, setBgColor] = useState("#0f172a");
  const [fgColor, setFgColor] = useState("#3b82f6");
  const [fgOpacity, setFgOpacity] = useState(50);
  const [copiedCSS, setCopiedCSS] = useState(false);
  const [copiedSVG, setCopiedSVG] = useState(false);

  const finalFgHex = useMemo(() => {
    let alpha = Math.round((fgOpacity / 100) * 255).toString(16);
    if (alpha.length === 1) alpha = `0${alpha}`;
    return `${fgColor}${alpha}`;
  }, [fgColor, fgOpacity]);

  const rawSvgBlob = useMemo(
    () => buildTileSvg(activePattern, bgColor, finalFgHex, size, fgSize),
    [activePattern, bgColor, finalFgHex, size, fgSize],
  );

  const exportSvg = useMemo(
    () => buildExportSvg(activePattern, bgColor, finalFgHex, size, fgSize),
    [activePattern, bgColor, finalFgHex, size, fgSize],
  );

  const encodedSvgUrl = useMemo(
    () => `data:image/svg+xml;utf8,${encodeURIComponent(rawSvgBlob)}`,
    [rawSvgBlob],
  );

  const cssValue = useMemo(
    () =>
      `background-color: ${bgColor};\nbackground-image: url("${encodedSvgUrl}");`,
    [bgColor, encodedSvgUrl],
  );

  const copy = (text: string, type: "css" | "svg") => {
    navigator.clipboard.writeText(text);
    if (type === "css") {
      setCopiedCSS(true);
      setTimeout(() => setCopiedCSS(false), 1200);
      return;
    }

    setCopiedSVG(true);
    setTimeout(() => setCopiedSVG(false), 1200);
  };

  const downloadSvg = () => {
    const blob = new Blob([exportSvg], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${activePattern}.svg`;
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 500);
  };

  return (
    <div className="w-full space-y-8 px-2 py-8 md:px-0">
      <div className="mb-2 flex flex-col gap-2 md:flex-row md:items-end md:justify-between">
        <h2 className="text-2xl font-bold tracking-tight text-white/90 md:text-3xl">
          SVG Pattern Generator
        </h2>
        <span className="text-xs text-white/40">
          Seamless SVG backgrounds for your UI
        </span>
      </div>

      <div className="flex w-full flex-col gap-8 xl:flex-row xl:items-stretch">
        <Card className="overflow-hidden rounded-3xl border border-white/10 shadow-xl xl:min-w-0 xl:flex-[1.2]">
          <CardContent
            className="relative flex h-[420px] items-center justify-center transition-all md:h-[500px]"
            style={{
              backgroundColor: bgColor,
              backgroundImage: `url("${encodedSvgUrl}")`,
            }}
          >
            <div className="absolute right-4 top-4 z-10">
              <Button
                size="sm"
                onClick={downloadSvg}
                className="border border-white/20 bg-white/10 text-white shadow hover:bg-white/20"
              >
                <Download className="mr-2 size-4" />
                Export SVG
              </Button>
            </div>
          </CardContent>
          <div className="flex items-center justify-between border-t border-white/10 bg-gradient-to-r from-white/5 to-white/0 p-4">
            <span className="text-sm font-medium capitalize tracking-wide opacity-70">
              {activePattern}
            </span>
            <span className="text-xs text-white/40">
              {size}x{size}px
            </span>
          </div>
        </Card>

        <Card className="space-y-7 rounded-3xl border border-white/10 bg-gradient-to-br from-white/5 to-white/0 p-6 xl:min-w-0 xl:flex-[1]">
          <CardTitle className="mb-2 flex items-center gap-2 text-base">
            <SlidersHorizontal className="size-4" />
            Controls
          </CardTitle>

          <div className="mb-2 grid grid-cols-3 gap-2">
            {(Object.keys(PATTERNS) as PatternType[]).map((pattern) => {
              const preview = buildTileSvg(
                pattern,
                "transparent",
                finalFgHex,
                24,
                1,
              );
              return (
                <Button
                  variant={activePattern === pattern ? "default" : "outline"}
                  key={pattern}
                  onClick={() => setActivePattern(pattern)}
                  className={`flex flex-col items-center gap-1 rounded-lg border-2 p-2 text-xs font-medium capitalize transition ${
                    activePattern === pattern
                      ? "border-primary bg-primary/90 text-white shadow"
                      : "border-white/10 bg-white/5 text-white/80 hover:bg-white/10"
                  }`}
                  style={{ minHeight: 54 }}
                >
                  <span className="mb-1 flex h-7 w-7 items-center justify-center rounded bg-white/10">
                    <span dangerouslySetInnerHTML={{ __html: preview }} />
                  </span>
                  {pattern}
                </Button>
              );
            })}
          </div>

          <div className="space-y-4 text-sm">
            <div>
              <div className="mb-1 flex justify-between">
                <span>Scale</span>
                <span className="font-mono">{size}px</span>
              </div>
              <Slider
                min={8}
                max={128}
                step={2}
                value={size}
                onChange={(e: SliderChangeEvent) =>
                  setSize(Number(e.target.value))
                }
              />
            </div>

            {activePattern !== "checker" && (
              <div>
                <div className="mb-1 flex justify-between">
                  <span>Width</span>
                  <span className="font-mono">{fgSize}x</span>
                </div>
                <Slider
                  min={0.5}
                  max={3}
                  step={0.1}
                  value={fgSize}
                  onChange={(e: SliderChangeEvent) =>
                    setFgSize(Number(e.target.value))
                  }
                />
              </div>
            )}

            <div>
              <div className="mb-1 flex justify-between">
                <span>Opacity</span>
                <span className="font-mono">{fgOpacity}%</span>
              </div>
              <Slider
                min={0}
                max={100}
                value={fgOpacity}
                onChange={(e: SliderChangeEvent) =>
                  setFgOpacity(Number(e.target.value))
                }
              />
            </div>
          </div>

          <div className="mt-2 flex gap-6">
            <div className="flex flex-col items-center gap-1 text-xs">
              <span className="mb-1 font-semibold text-white/70">BG</span>
              <ColorInput
                value={bgColor}
                onChange={(e) => setBgColor(e.target.value)}
                className="h-10 w-10 appearance-none overflow-hidden rounded-full border-2 border-white/20 bg-transparent p-0 shadow"
                style={{ borderRadius: "50%" }}
                aria-label="Background color"
              />
            </div>
            <div className="flex flex-col items-center gap-1 text-xs">
              <span className="mb-1 font-semibold text-white/70">FG</span>
              <ColorInput
                value={fgColor}
                onChange={(e) => setFgColor(e.target.value)}
                className="h-10 w-10 appearance-none overflow-hidden rounded-full border-2 border-white/20 bg-transparent p-0 shadow"
                style={{ borderRadius: "50%" }}
                aria-label="Foreground color"
              />
            </div>
          </div>
        </Card>

        <div className="flex min-w-0 flex-col gap-4 xl:flex-[1]">
          <Card className="border border-white/10 bg-gradient-to-br from-white/5 to-white/0 shadow-lg">
            <CardHeader className="flex items-center justify-between pb-2">
              <CardTitle className="text-base">CSS</CardTitle>
              <Button
                size="sm"
                onClick={() => copy(cssValue, "css")}
                className={
                  copiedCSS
                    ? "bg-white/20 text-white"
                    : "bg-white/10 text-white hover:bg-white/20"
                }
              >
                {copiedCSS ? (
                  <Check className="size-4" />
                ) : (
                  <CopyIcon className="size-4" />
                )}
              </Button>
            </CardHeader>
            <Textarea
              readOnly
              value={cssValue}
              className="min-h-[140px] rounded-xl bg-[#05070f] font-mono text-xs text-rose-400"
            />
          </Card>

          <Card className="border border-white/10 bg-gradient-to-br from-white/5 to-white/0 shadow-lg">
            <CardHeader className="flex items-center justify-between pb-2">
              <CardTitle className="text-base">SVG</CardTitle>
              <Button
                size="sm"
                onClick={() => copy(exportSvg, "svg")}
                className={
                  copiedSVG
                    ? "bg-white/20 text-white"
                    : "bg-white/10 text-white hover:bg-white/20"
                }
              >
                {copiedSVG ? (
                  <Check className="size-4" />
                ) : (
                  <CopyIcon className="size-4" />
                )}
              </Button>
            </CardHeader>
            <Textarea
              readOnly
              value={exportSvg}
              className="min-h-[220px] rounded-xl bg-[#05070f] font-mono text-xs text-amber-400"
            />
          </Card>
        </div>
      </div>
    </div>
  );
}
