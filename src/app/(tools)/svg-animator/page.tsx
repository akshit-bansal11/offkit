"use client";

import DOMPurify from "dompurify";
import { Download, Play } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { OutputField } from "@/components/ui/design/OutputField";
import { Badge } from "@/components/ui/feedback/Badge";
import { Checkbox } from "@/components/ui/form/Checkbox";
import { ColorInput } from "@/components/ui/form/ColorInput";
import { Input } from "@/components/ui/form/Input";
import { Label } from "@/components/ui/form/Label";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("svg-animator");

export default function SvgAnimatorPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <SvgAnimatorTool />
    </ToolPageShell>
  );
}

type PlayMode = "sequential" | "simultaneous";

interface ParsedSvgData {
  fileName: string;
  sourceSvg: string;
  pathLengths: number[];
}

const MIN_SPEED = 0.01;
const MAX_SPEED = 5;
const DEFAULT_SPEED = 0.5;
const MIN_ZOOM = 0.25;
const MAX_ZOOM = 3;
const DEFAULT_ZOOM = 1;

const MIN_STROKE_WIDTH = 0.5;
const MAX_STROKE_WIDTH = 10;
const DEFAULT_STROKE_WIDTH = 1.5;
const DEFAULT_STROKE_COLOR = "#000000";

function speedToDuration(speed: number): number {
  return 1 / speed;
}

function isSvgFile(file: File) {
  return (
    file.type === "image/svg+xml" || file.name.toLowerCase().endsWith(".svg")
  );
}

function readSvgText(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = (event) => {
      const result = event.target?.result;
      if (typeof result !== "string") {
        reject(new Error("Could not read file contents as text."));
        return;
      }
      resolve(result);
    };

    reader.onerror = () => {
      reject(new Error("Failed to read the SVG file."));
    };

    reader.readAsText(file);
  });
}

function measurePathLengths(svg: SVGSVGElement) {
  const holder = document.createElement("div");
  holder.style.position = "fixed";
  holder.style.left = "-99999px";
  holder.style.top = "-99999px";
  holder.style.width = "0";
  holder.style.height = "0";
  holder.style.opacity = "0";
  holder.style.pointerEvents = "none";

  const clone = svg.cloneNode(true) as SVGSVGElement;
  holder.appendChild(clone);
  document.body.appendChild(holder);

  const paths = Array.from(clone.querySelectorAll("path"));
  const lengths = paths.map((path) => {
    try {
      const totalLength = path.getTotalLength();
      return Number.isFinite(totalLength) && totalLength > 0
        ? Number(totalLength.toFixed(3))
        : 1;
    } catch {
      return 1;
    }
  });

  document.body.removeChild(holder);
  return lengths;
}

function buildAnimatedSvg(
  sourceSvg: string,
  pathLengths: number[],
  durationPerPath: number,
  mode: PlayMode,
  strokeColor: string,
  strokeWidth: number,
  hasFill: boolean,
) {
  const doc = new DOMParser().parseFromString(sourceSvg, "image/svg+xml");
  const parserError = doc.querySelector("parsererror");
  if (parserError) {
    return "";
  }

  const svg = doc.querySelector("svg");
  if (!svg) {
    return "";
  }

  svg
    .querySelectorAll(
      "style[data-svg-animator='true'], animate[data-svg-animator='true']",
    )
    .forEach((node) => {
      node.remove();
    });

  const paths = Array.from(svg.querySelectorAll("path"));
  paths.forEach((path) => {
    const existingAnimations = path.querySelectorAll(
      "animate[data-svg-animator='true']",
    );

    existingAnimations.forEach((node) => {
      node.remove();
    });
  });

  paths.forEach((path, index) => {
    const length = Math.max(pathLengths[index] ?? 1, 1);
    const delay = mode === "sequential" ? index * durationPerPath : 0;

    path.removeAttribute("stroke");
    path.removeAttribute("stroke-width");
    path.removeAttribute("stroke-dasharray");
    path.removeAttribute("stroke-dashoffset");

    path.setAttribute("stroke", strokeColor);
    path.setAttribute("stroke-width", String(strokeWidth));
    path.setAttribute("stroke-dasharray", String(length));
    path.setAttribute("stroke-dashoffset", String(length));

    if (!hasFill) {
      path.setAttribute("fill", "none");
    }

    const animate = doc.createElementNS(
      "http://www.w3.org/2000/svg",
      "animate",
    );

    animate.setAttribute("data-svg-animator", "true");
    animate.setAttribute("attributeName", "stroke-dashoffset");
    animate.setAttribute("from", String(length));
    animate.setAttribute("to", "0");
    animate.setAttribute("dur", `${durationPerPath}s`);
    animate.setAttribute("begin", `${delay}s`);
    animate.setAttribute("fill", "freeze");
    animate.setAttribute("calcMode", "spline");
    animate.setAttribute("keySplines", "0.25 0.1 0.25 1");
    animate.setAttribute("keyTimes", "0;1");

    path.appendChild(animate);
  });

  svg.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  svg.setAttribute("xmlns:xlink", "http://www.w3.org/1999/xlink");

  return new XMLSerializer().serializeToString(svg);
}

function formatDuration(seconds: number) {
  return `${seconds.toFixed(2)}s`;
}

function SvgAnimatorTool() {
  const [parsedSvg, setParsedSvg] = useState<ParsedSvgData | null>(null);
  const [mode, setMode] = useState<PlayMode>("sequential");
  const [speed, setSpeed] = useState(DEFAULT_SPEED);
  const [strokeColor, setStrokeColor] = useState(DEFAULT_STROKE_COLOR);
  const [strokeWidth, setStrokeWidth] = useState(DEFAULT_STROKE_WIDTH);
  const [hasFill, setHasFill] = useState(true);
  const [zoom, setZoom] = useState(DEFAULT_ZOOM);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [replayTick, setReplayTick] = useState(0);

  const durationPerPath = useMemo(() => speedToDuration(speed), [speed]);

  const animatedSvg = useMemo(() => {
    if (!parsedSvg) {
      return "";
    }

    return buildAnimatedSvg(
      parsedSvg.sourceSvg,
      parsedSvg.pathLengths,
      durationPerPath,
      mode,
      strokeColor,
      strokeWidth,
      hasFill,
    );
  }, [parsedSvg, durationPerPath, mode, strokeColor, strokeWidth, hasFill]);

  const totalAnimationTime = useMemo(() => {
    if (!parsedSvg) {
      return 0;
    }

    return mode === "sequential"
      ? parsedSvg.pathLengths.length * durationPerPath
      : durationPerPath;
  }, [parsedSvg, durationPerPath, mode]);

  const previewKey = useMemo(() => {
    if (!parsedSvg) {
      return "empty";
    }

    return `${parsedSvg.fileName}-${mode}-${speed}-${strokeColor}-${strokeWidth}-${hasFill}-${replayTick}`;
  }, [parsedSvg, mode, speed, strokeColor, strokeWidth, hasFill, replayTick]);

  const handleFile = useCallback(async (file: File) => {
    if (!isSvgFile(file)) {
      setErrorMessage("Please upload a valid SVG file.");
      return;
    }

    try {
      const text = await readSvgText(file);
      const doc = new DOMParser().parseFromString(text, "image/svg+xml");
      const parserError = doc.querySelector("parsererror");
      if (parserError) {
        throw new Error("This SVG could not be parsed.");
      }

      const svg = doc.querySelector("svg");
      if (!svg) {
        throw new Error("No <svg> root element was found.");
      }

      // The file is untrusted and its markup ends up in the page, so scripts, event handlers and
      // javascript: links are stripped before anything measures or renders it. Sanitising the parsed
      // tree in place avoids an HTML round trip, which would break valid XML such as &#160;.
      // <use> is kept: icon files rely on it, and the app cannot load external references anyway.
      DOMPurify.sanitize(svg, {
        USE_PROFILES: { svg: true, svgFilters: true },
        ADD_TAGS: ["use"],
        IN_PLACE: true,
      });

      const pathElements = Array.from(svg.querySelectorAll("path"));
      if (pathElements.length === 0) {
        throw new Error("No <path> elements were found in this SVG.");
      }

      const lengths = measurePathLengths(svg as SVGSVGElement);
      const sourceSvg = new XMLSerializer().serializeToString(svg);

      setParsedSvg({
        fileName: file.name,
        sourceSvg,
        pathLengths: lengths,
      });
      setErrorMessage(null);
      setReplayTick((current) => current + 1);
    } catch (error) {
      setParsedSvg(null);
      setErrorMessage(
        error instanceof Error
          ? error.message
          : "Failed to process the SVG file.",
      );
    }
  }, []);

  const handleReplay = useCallback(() => {
    setReplayTick((current) => current + 1);
  }, []);

  const handleDownload = useCallback(() => {
    if (!animatedSvg) {
      return;
    }

    const blob = new Blob([animatedSvg], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${parsedSvg?.fileName.replace(/\.svg$/i, "") ?? "animated"}-animated.svg`;
    link.click();
    URL.revokeObjectURL(url);
  }, [animatedSvg, parsedSvg]);

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="an SVG file"
        supportedFormats="SVG"
        accept=".svg,image/svg+xml"
        onFilesSelected={(incoming) => {
          const file = incoming.find((entry) => isSvgFile(entry));

          if (!file) {
            setErrorMessage("Drop an SVG file to continue.");
            return;
          }

          void handleFile(file);
        }}
      />

      {errorMessage ? (
        <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {errorMessage}
        </div>
      ) : null}

      {parsedSvg ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Summary</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              <Badge className="border-white/15 bg-white/[0.06] text-white/75">
                {parsedSvg.pathLengths.length} paths
              </Badge>
              <Badge className="border-info/25 bg-info/10 text-info">
                Mode: {mode}
              </Badge>
              <Badge className="border-success/25 bg-success/10 text-success">
                Speed: {speed.toFixed(2)}x
              </Badge>
              <Badge className="border-warning/25 bg-warning/10 text-warning">
                Total: {formatDuration(totalAnimationTime)}
              </Badge>
            </div>
          </CardContent>
        </Card>
      ) : null}

      {parsedSvg && animatedSvg ? (
        <div className="grid gap-6 lg:grid-cols-3">
          {/* Controls */}
          <Card className="lg:col-span-1">
            <CardHeader>
              <CardTitle className="text-sm">Controls</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label className="text-xs">Mode</Label>
                <div className="flex gap-1">
                  <Button
                    type="button"
                    variant={mode === "sequential" ? "default" : "outline"}
                    onClick={() => setMode("sequential")}
                    size="sm"
                    className="flex-1 text-xs"
                  >
                    Sequential
                  </Button>
                  <Button
                    type="button"
                    variant={mode === "simultaneous" ? "default" : "outline"}
                    onClick={() => setMode("simultaneous")}
                    size="sm"
                    className="flex-1 text-xs"
                  >
                    Simultaneous
                  </Button>
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs">Speed: {speed.toFixed(2)}x</Label>
                <Slider
                  value={speed}
                  min={MIN_SPEED}
                  max={MAX_SPEED}
                  step={0.01}
                  onChange={(event) => {
                    const next = Number(event.currentTarget.value);
                    setSpeed(next);
                  }}
                  disabled={!parsedSvg}
                />
              </div>

              <div className="space-y-2">
                <Label className="text-xs">Stroke Color</Label>
                <div className="flex items-center gap-2">
                  <ColorInput
                    value={strokeColor}
                    onChange={(event) => setStrokeColor(event.target.value)}
                    disabled={!parsedSvg}
                    className="h-8 w-12 rounded border border-white/20 bg-background"
                  />
                  <Input
                    value={strokeColor}
                    onChange={(event) => setStrokeColor(event.target.value)}
                    disabled={!parsedSvg}
                    placeholder="#000000"
                    className="h-8 flex-1 text-xs"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs">
                  Width: {strokeWidth.toFixed(1)}
                </Label>
                <Slider
                  value={strokeWidth}
                  min={MIN_STROKE_WIDTH}
                  max={MAX_STROKE_WIDTH}
                  step={0.1}
                  onChange={(event) => {
                    const next = Number(event.currentTarget.value);
                    setStrokeWidth(next);
                  }}
                  disabled={!parsedSvg}
                />
              </div>

              <div className="space-y-2">
                <Label className="text-xs">
                  Zoom: {Math.round(zoom * 100)}%
                </Label>
                <Slider
                  value={zoom}
                  min={MIN_ZOOM}
                  max={MAX_ZOOM}
                  step={0.05}
                  onChange={(event) => {
                    const next = Number(event.currentTarget.value);
                    setZoom(next);
                  }}
                  disabled={!parsedSvg}
                />
              </div>

              <div className="flex items-center gap-2">
                <Checkbox
                  id="fill"
                  checked={hasFill}
                  onCheckedChange={setHasFill}
                  disabled={!parsedSvg}
                  className="h-3 w-3"
                />
                <Label htmlFor="fill" className="cursor-pointer text-xs">
                  Fill?
                </Label>
              </div>

              <div className="flex gap-2 pt-2">
                <Button
                  onClick={handleReplay}
                  disabled={!animatedSvg}
                  size="sm"
                  className="flex-1 text-xs"
                >
                  <Play className="size-3" />
                  Replay
                </Button>
                <Button
                  onClick={handleDownload}
                  variant="outline"
                  disabled={!animatedSvg}
                  size="sm"
                  className="flex-1 text-xs"
                >
                  <Download className="size-3" />
                  Download
                </Button>
              </div>
            </CardContent>
          </Card>

          {/* Preview */}
          <Card className="lg:col-span-2">
            <CardHeader>
              <CardTitle className="text-sm">Preview</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="overflow-auto rounded-2xl border border-white/10 bg-background/40 p-6">
                <div
                  key={previewKey}
                  className="flex min-h-[420px] items-center justify-center text-foreground [&_svg]:h-auto [&_svg]:max-h-none [&_svg]:max-w-none"
                  style={{
                    transform: `scale(${zoom})`,
                    transformOrigin: "center",
                  }}
                  dangerouslySetInnerHTML={{ __html: animatedSvg }}
                />
              </div>
            </CardContent>
          </Card>
        </div>
      ) : parsedSvg ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Preview</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex min-h-[220px] items-center justify-center rounded-2xl border border-dashed border-white/20 bg-background/40 text-sm text-muted-foreground">
              Building animation...
            </div>
          </CardContent>
        </Card>
      ) : null}

      {animatedSvg ? (
        <OutputField
          label="Animated SVG Markup"
          description="Export-ready SVG with embedded @keyframes and per-path animation styles."
          value={animatedSvg}
        />
      ) : null}
    </div>
  );
}
