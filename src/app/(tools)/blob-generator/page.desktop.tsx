"use client";

import { CheckCircle2, Copy, Dices, SlidersVertical } from "lucide-react";
import React, { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Checkbox } from "@/components/ui/form/Checkbox";
import { ColorInput } from "@/components/ui/form/ColorInput";
import { Label } from "@/components/ui/form/Label";
import { Textarea } from "@/components/ui/form/Textarea";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("blob-generator");

export default function BlobGeneratorPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <BlobGeneratorTool />
    </ToolPageShell>
  );
}

type CornerKey = "tl" | "tr" | "br" | "bl";
type Axis = "x" | "y";
type PointMode = "four" | "eight";

interface CornerRadius {
  x: number;
  y: number;
}

type CornerMap = Record<CornerKey, CornerRadius>;

const INITIAL_CORNERS: CornerMap = {
  tl: { x: 30, y: 30 },
  tr: { x: 70, y: 30 },
  br: { x: 70, y: 70 },
  bl: { x: 30, y: 70 },
};

const CORNER_LABELS: Record<CornerKey, string> = {
  tl: "Top-Left",
  tr: "Top-Right",
  br: "Bottom-Right",
  bl: "Bottom-Left",
};

const DIAGONAL_PAIR: Record<CornerKey, CornerKey> = {
  tl: "br",
  tr: "bl",
  br: "tl",
  bl: "tr",
};

const OPPOSITE_BY_AXIS: Record<Axis, Record<CornerKey, CornerKey>> = {
  x: { tl: "tr", tr: "tl", br: "bl", bl: "br" },
  y: { tl: "bl", tr: "br", br: "tr", bl: "tl" },
};

function clampPercent(value: number) {
  return Math.max(0, Math.min(100, Math.round(value)));
}

function cloneCorners(input: CornerMap): CornerMap {
  return {
    tl: { ...input.tl },
    tr: { ...input.tr },
    br: { ...input.br },
    bl: { ...input.bl },
  };
}

function constrainPairByCorner(next: CornerMap, corner: CornerKey, axis: Axis) {
  const opposite = OPPOSITE_BY_AXIS[axis][corner];
  const maxOpposite = Math.max(0, 100 - next[corner][axis]);
  if (next[opposite][axis] > maxOpposite) {
    next[opposite][axis] = clampPercent(maxOpposite);
  }
}

function constrainAllPairs(next: CornerMap) {
  constrainPairByCorner(next, "tl", "x");
  constrainPairByCorner(next, "bl", "x");
  constrainPairByCorner(next, "tl", "y");
  constrainPairByCorner(next, "tr", "y");
  return next;
}

function sanitizeCorners(input: CornerMap): CornerMap {
  const next = cloneCorners(input);
  (Object.keys(next) as CornerKey[]).forEach((corner) => {
    next[corner].x = clampPercent(next[corner].x);
    next[corner].y = clampPercent(next[corner].y);
  });
  return constrainAllPairs(next);
}

function randomCorners() {
  const rand = () => Math.floor(Math.random() * 81) + 10;
  return sanitizeCorners({
    tl: { x: rand(), y: rand() },
    tr: { x: rand(), y: rand() },
    br: { x: rand(), y: rand() },
    bl: { x: rand(), y: rand() },
  });
}

function cornersToRadius(corners: CornerMap) {
  return `${corners.tl.x}% ${corners.tr.x}% ${corners.br.x}% ${corners.bl.x}% / ${corners.tl.y}% ${corners.tr.y}% ${corners.br.y}% ${corners.bl.y}%`;
}

interface CornerControlProps {
  corner: CornerKey;
  values: CornerRadius;
  mode: PointMode;
  onChange: (corner: CornerKey, axis: Axis, value: number) => void;
  onChangeBoth: (corner: CornerKey, value: number) => void;
}

interface RadiusSliderProps {
  value: number;
  onChange: (value: number) => void;
}

function RadiusSlider({ value, onChange }: RadiusSliderProps) {
  const marks = [25, 50, 75] as const;

  return (
    <div className="space-y-2">
      <div className="relative">
        <Slider
          min={0}
          max={100}
          step={1}
          value={value}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
            onChange(Number(e.target.value))
          }
        />
        {marks.map((mark) => (
          <Button
            key={mark}
            type="button"
            onClick={() => onChange(mark)}
            variant="ghost"
            size="icon"
            className="absolute top-1/2 h-3 w-3 -translate-y-1/2 rounded-full border border-white/40 bg-background/80 p-0 shadow-sm hover:bg-primary/30"
            style={{ left: `calc(${mark}% - 6px)` }}
            aria-label={`Set radius to ${mark}%`}
            title={`${mark}%`}
          />
        ))}
      </div>
      <div className="flex items-center justify-around text-[10px] font-mono text-muted-foreground">
        {marks.map((mark) => (
          <Button
            key={mark}
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onChange(mark)}
            className="h-auto rounded px-1 py-0.5 text-[10px] font-normal text-muted-foreground hover:bg-white/10"
          >
            {mark}%
          </Button>
        ))}
      </div>
    </div>
  );
}

function CornerControl({
  corner,
  values,
  mode,
  onChange,
  onChangeBoth,
}: CornerControlProps) {
  return (
    <div className="space-y-4 rounded-2xl border border-white/5 bg-background/30 p-4">
      <h4 className="mb-4 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        {CORNER_LABELS[corner]} Corner
      </h4>

      {mode === "four" ? (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-sm font-medium text-foreground">
            Radius
            <span className="rounded bg-background/50 px-2 py-0.5 font-mono text-xs text-muted-foreground">
              {values.x}%
            </span>
          </div>
          <RadiusSlider
            value={values.x}
            onChange={(v) => onChangeBoth(corner, v)}
          />
        </div>
      ) : (
        <>
          <div className="space-y-4">
            <div className="flex items-center justify-between text-sm font-medium text-foreground">
              X Radius
              <span className="rounded bg-background/50 px-2 py-0.5 font-mono text-xs text-muted-foreground">
                {values.x}%
              </span>
            </div>
            <RadiusSlider
              value={values.x}
              onChange={(v) => onChange(corner, "x", v)}
            />
          </div>

          <div className="space-y-4 pt-2">
            <div className="flex items-center justify-between text-sm font-medium text-foreground">
              Y Radius
              <span className="rounded bg-background/50 px-2 py-0.5 font-mono text-xs text-muted-foreground">
                {values.y}%
              </span>
            </div>
            <RadiusSlider
              value={values.y}
              onChange={(v) => onChange(corner, "y", v)}
            />
          </div>
        </>
      )}
    </div>
  );
}

function BlobGeneratorTool() {
  const [corners, setCorners] = useState<CornerMap>(INITIAL_CORNERS);
  const [pointMode, setPointMode] = useState<PointMode>("eight");
  const [lockAxis, setLockAxis] = useState(false);
  const [mirrorDiagonals, setMirrorDiagonals] = useState(false);
  const [blobColor, setBlobColor] = useState("#8b5cf6");
  const [blobGradient, setBlobGradient] = useState(true);
  const [copiedCSS, setCopiedCSS] = useState(false);
  const [copiedTW, setCopiedTW] = useState(false);

  const borderRadiusValue = useMemo(() => cornersToRadius(corners), [corners]);

  const cssValue = useMemo(
    () =>
      [
        `border-radius: ${borderRadiusValue};`,
        blobGradient
          ? `background: linear-gradient(135deg, ${blobColor}, ${blobColor}80);`
          : `background: ${blobColor};`,
      ].join("\n"),
    [borderRadiusValue, blobColor, blobGradient],
  );

  const tailwindValue = useMemo(
    () =>
      `rounded-[${borderRadiusValue.replace(/ /g, "_")}] ${
        blobGradient
          ? "bg-gradient-to-br from-violet-500 to-violet-500/50"
          : "bg-violet-500"
      }`,
    [borderRadiusValue, blobGradient],
  );

  function updateCorner(corner: CornerKey, axis: Axis, rawValue: number) {
    setCorners((prev) => {
      const value = clampPercent(rawValue);
      const lockBoth = lockAxis || pointMode === "four";
      const next = cloneCorners(prev);

      next[corner][axis] = value;
      if (lockBoth) {
        next[corner].x = value;
        next[corner].y = value;
      }

      if (mirrorDiagonals) {
        const pair = DIAGONAL_PAIR[corner];
        if (lockBoth) {
          next[pair].x = value;
          next[pair].y = value;
        } else {
          next[pair][axis] = value;
        }
      }

      if (lockBoth) {
        constrainPairByCorner(next, corner, "x");
        constrainPairByCorner(next, corner, "y");
        if (mirrorDiagonals) {
          const pair = DIAGONAL_PAIR[corner];
          constrainPairByCorner(next, pair, "x");
          constrainPairByCorner(next, pair, "y");
        }
      } else {
        constrainPairByCorner(next, corner, axis);
        if (mirrorDiagonals) {
          constrainPairByCorner(next, DIAGONAL_PAIR[corner], axis);
        }
      }

      return next;
    });
  }

  function updateCornerBoth(corner: CornerKey, rawValue: number) {
    const value = clampPercent(rawValue);
    setCorners((prev) => {
      const next = cloneCorners(prev);

      next[corner].x = value;
      next[corner].y = value;

      if (mirrorDiagonals) {
        const pair = DIAGONAL_PAIR[corner];
        next[pair].x = value;
        next[pair].y = value;
      }

      constrainPairByCorner(next, corner, "x");
      constrainPairByCorner(next, corner, "y");
      if (mirrorDiagonals) {
        const pair = DIAGONAL_PAIR[corner];
        constrainPairByCorner(next, pair, "x");
        constrainPairByCorner(next, pair, "y");
      }

      return next;
    });
  }

  function applyPreset(type: "smooth" | "organic" | "wild") {
    if (type === "smooth") {
      setCorners(
        sanitizeCorners({
          tl: { x: 42, y: 38 },
          tr: { x: 58, y: 36 },
          br: { x: 56, y: 62 },
          bl: { x: 44, y: 64 },
        }),
      );
      return;
    }

    if (type === "organic") {
      setCorners(
        sanitizeCorners({
          tl: { x: 28, y: 48 },
          tr: { x: 72, y: 22 },
          br: { x: 60, y: 78 },
          bl: { x: 40, y: 52 },
        }),
      );
      return;
    }

    setCorners(
      sanitizeCorners({
        tl: { x: 12, y: 84 },
        tr: { x: 88, y: 16 },
        br: { x: 76, y: 92 },
        bl: { x: 24, y: 8 },
      }),
    );
  }

  function randomize() {
    setCorners(randomCorners());
  }

  const setCssCopy = () => {
    navigator.clipboard.writeText(cssValue);
    setCopiedCSS(true);
    setTimeout(() => setCopiedCSS(false), 2000);
  };

  const setTwCopy = () => {
    navigator.clipboard.writeText(tailwindValue);
    setCopiedTW(true);
    setTimeout(() => setCopiedTW(false), 2000);
  };

  return (
    <div className="mx-auto max-w-7xl space-y-6">
      <div className="grid gap-6 lg:grid-cols-[2fr_1.5fr]">
        <div className="flex flex-col space-y-6">
          <Card className="flex min-h-[500px] flex-1 flex-col overflow-hidden rounded-3xl border-white/10 bg-card/70 shadow-sm">
            <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 bg-background/20 px-6 py-4">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Live Shape Projection
              </CardTitle>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => applyPreset("smooth")}
                  className="h-8 bg-background/50 text-xs leading-none"
                >
                  Smooth
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => applyPreset("organic")}
                  className="h-8 bg-background/50 text-xs leading-none"
                >
                  Organic
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => applyPreset("wild")}
                  className="h-8 bg-background/50 text-xs leading-none"
                >
                  Wild
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={randomize}
                  className="h-8 gap-1.5 bg-background/50 text-xs leading-none"
                >
                  <Dices className="size-3.5" />
                  Randomize
                </Button>
              </div>
            </CardHeader>
            <CardContent className="relative flex flex-1 items-center justify-center bg-[#1e293b]/50 p-8 sm:p-16 dotted-bg">
              <div
                className="pointer-events-none absolute inset-0"
                style={{
                  backgroundImage:
                    "radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)",
                  backgroundSize: "32px 32px",
                }}
              />

              <div
                className="relative z-10 aspect-square w-full max-w-[280px] shadow-2xl transition-all duration-[320ms] ease-out"
                style={{
                  borderRadius: borderRadiusValue,
                  background: blobGradient
                    ? `linear-gradient(135deg, ${blobColor}, ${blobColor}40)`
                    : blobColor,
                }}
              />
            </CardContent>
          </Card>

          <div className="grid gap-4 sm:grid-cols-2">
            <Card className="overflow-hidden rounded-3xl border-white/10 bg-card/70 shadow-sm">
              <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 bg-background/20 px-4 py-3">
                <CardTitle className="flex items-center gap-2 border-l-2 border-blue-500 pl-2 text-sm font-medium text-muted-foreground">
                  Native CSS
                </CardTitle>
                <Button
                  variant={copiedCSS ? "default" : "secondary"}
                  size="sm"
                  onClick={setCssCopy}
                  className={`h-7 px-3 text-xs transition-colors ${copiedCSS ? "bg-white/20 text-white hover:bg-white/25" : ""}`}
                >
                  {copiedCSS ? (
                    <>
                      <CheckCircle2 className="mr-2 size-3.5" />
                      Copied
                    </>
                  ) : (
                    <>
                      <Copy className="mr-2 size-3.5" />
                      Copy Native
                    </>
                  )}
                </Button>
              </CardHeader>
              <Textarea
                readOnly
                value={cssValue}
                className="h-[90px] resize-none border-0 bg-background/50 p-4 font-mono text-sm leading-relaxed text-blue-400 focus-visible:ring-0"
              />
            </Card>

            <Card className="overflow-hidden rounded-3xl border-white/10 bg-card/70 shadow-sm">
              <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 bg-background/20 px-4 py-3">
                <CardTitle className="flex items-center gap-2 border-l-2 border-white/20 pl-2 text-sm font-medium text-muted-foreground">
                  Tailwind JIT
                </CardTitle>
                <Button
                  variant={copiedTW ? "default" : "secondary"}
                  size="sm"
                  onClick={setTwCopy}
                  className={`h-7 px-3 text-xs transition-colors ${copiedTW ? "bg-white/20 text-white hover:bg-white/25" : ""}`}
                >
                  {copiedTW ? (
                    <>
                      <CheckCircle2 className="mr-2 size-3.5" />
                      Copied
                    </>
                  ) : (
                    <>
                      <Copy className="mr-2 size-3.5" />
                      Copy Arbitrary
                    </>
                  )}
                </Button>
              </CardHeader>
              <Textarea
                readOnly
                value={tailwindValue}
                className="h-[90px] resize-none border-0 bg-background/50 p-4 font-mono text-[13px] leading-relaxed text-white/80 focus-visible:ring-0"
              />
            </Card>
          </div>
        </div>

        <div className="flex h-full flex-col space-y-6">
          <Card className="relative flex flex-1 flex-col overflow-hidden rounded-3xl border-white/10 bg-card/70 shadow-sm">
            <CardHeader className="sticky top-0 z-10 border-b border-white/5 bg-background/20 py-4 pb-0">
              <CardTitle className="flex items-center gap-2 pb-4 text-base">
                <SlidersVertical className="size-4 text-primary" />
                Blob Controls
              </CardTitle>
            </CardHeader>

            <div className="flex-1 space-y-8 overflow-y-auto p-6 custom-scrollbar">
              <div className="rounded-2xl border border-white/5 bg-background/30 p-4">
                <div className="grid gap-4 sm:grid-cols-2">
                  <div className="rounded-xl border border-white/5 bg-background/40 p-2 sm:col-span-2">
                    <SegmentedControl
                      variant="dark"
                      size="sm"
                      value={pointMode}
                      onValueChange={(value) =>
                        setPointMode(value as PointMode)
                      }
                      options={[
                        { label: "4-Point", value: "four" },
                        { label: "8-Point", value: "eight" },
                      ]}
                    />
                  </div>
                  <Label className="flex items-center justify-between rounded-xl border border-white/5 bg-background/40 px-3 py-2 text-sm font-normal">
                    <span>Lock X/Y per corner</span>
                    <Checkbox
                      checked={lockAxis}
                      onCheckedChange={setLockAxis}
                    />
                  </Label>
                  <Label className="flex items-center justify-between rounded-xl border border-white/5 bg-background/40 px-3 py-2 text-sm font-normal">
                    <span>Mirror diagonals</span>
                    <Checkbox
                      checked={mirrorDiagonals}
                      onCheckedChange={setMirrorDiagonals}
                    />
                  </Label>
                </div>
                <p className="mt-3 text-xs text-muted-foreground">
                  Corner values are automatically normalized to keep side sums
                  valid, so x/y edits stay stable.
                </p>
              </div>

              <div className="grid gap-x-8 gap-y-6">
                <CornerControl
                  corner="tl"
                  values={corners.tl}
                  mode={pointMode}
                  onChange={updateCorner}
                  onChangeBoth={updateCornerBoth}
                />
                <CornerControl
                  corner="tr"
                  values={corners.tr}
                  mode={pointMode}
                  onChange={updateCorner}
                  onChangeBoth={updateCornerBoth}
                />
                <CornerControl
                  corner="br"
                  values={corners.br}
                  mode={pointMode}
                  onChange={updateCorner}
                  onChangeBoth={updateCornerBoth}
                />
                <CornerControl
                  corner="bl"
                  values={corners.bl}
                  mode={pointMode}
                  onChange={updateCorner}
                  onChangeBoth={updateCornerBoth}
                />
              </div>
            </div>

            <div className="sticky bottom-0 z-10 border-t border-white/5 bg-background/20 p-6 backdrop-blur-md">
              <div className="flex items-center justify-between">
                <Label className="block text-foreground">
                  Blob Context Fill
                </Label>
                <div className="flex items-center gap-4">
                  <Label className="flex items-center gap-2 font-normal">
                    <Checkbox
                      checked={blobGradient}
                      onCheckedChange={setBlobGradient}
                    />
                    <span className="select-none text-xs text-muted-foreground">
                      Gradient Depth
                    </span>
                  </Label>
                  <ColorInput
                    value={blobColor}
                    onChange={(e) => setBlobColor(e.target.value)}
                    className="block h-6 w-6 rounded border-0 bg-transparent p-0"
                  />
                </div>
              </div>
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}
