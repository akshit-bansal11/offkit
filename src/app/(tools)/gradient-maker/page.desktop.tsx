"use client";

import { Download, GripVertical, Layers3, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/design/CopyButton";
import { OutputField } from "@/components/ui/design/OutputField";
import { Badge } from "@/components/ui/feedback/Badge";
import { ColorInput } from "@/components/ui/form/ColorInput";
import { Input } from "@/components/ui/form/Input";
import { Label } from "@/components/ui/form/Label";
import { Select } from "@/components/ui/form/Select";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";
import {
  addStop,
  buildGradientCss,
  createDefaultGradientStops,
  downloadGradientPng,
  type GradientKind,
  gradientToBackgroundProperty,
  gradientToTailwindArbitrary,
  LINEAR_DIRECTION_PRESETS,
  moveStop,
  RADIAL_SHAPES,
} from "@/lib/design-tools/gradients";

const tool = getToolBySlug("gradient-maker");

export default function GradientMakerPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <GradientMakerTool />
    </ToolPageShell>
  );
}

function GradientMakerTool() {
  const [type, setType] = useState<GradientKind>("linear");
  const [linearAngle, setLinearAngle] = useState(135);
  const [radialShape, setRadialShape] = useState<string>(RADIAL_SHAPES[0]);
  const [conicFrom, setConicFrom] = useState(90);
  const [stops, setStops] = useState(createDefaultGradientStops);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [isDownloading, setIsDownloading] = useState(false);

  const gradientCss = buildGradientCss({
    type,
    stops,
    linearAngle,
    radialShape,
    conicFrom,
  });
  const backgroundProperty = gradientToBackgroundProperty(gradientCss);
  const tailwindArbitrary = gradientToTailwindArbitrary(gradientCss);

  async function handleDownload() {
    try {
      setIsDownloading(true);
      await downloadGradientPng(gradientCss, "gradient-maker-preview.png");
    } finally {
      setIsDownloading(false);
    }
  }

  return (
    <div className="grid gap-6 xl:grid-cols-[1.15fr_0.85fr]">
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle>Build Your Gradient</CardTitle>
              <p className="mt-2 text-sm text-muted-foreground">
                Add as many stops as you need, drag them to reorder, and tune
                the final direction or angle live.
              </p>
            </div>
            <Badge variant="secondary" className="gap-1.5">
              <Layers3 className="size-3.5" />
              {stops.length} stops
            </Badge>
          </div>

          <div className="grid gap-4 rounded-2xl border bg-background/50 p-4 md:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="gradient-type">Gradient Type</Label>
              <Select
                id="gradient-type"
                options={[
                  { label: "Linear", value: "linear" },
                  { label: "Radial", value: "radial" },
                  { label: "Conic", value: "conic" },
                ]}
                value={type}
                variant="dark"
                onChange={(event) =>
                  setType(event.target.value as GradientKind)
                }
              />
            </div>

            {type === "linear" ? (
              <div className="space-y-2">
                <Label htmlFor="linear-angle">Direction / Angle</Label>
                <Select
                  id="linear-angle"
                  options={LINEAR_DIRECTION_PRESETS.map((preset) => ({
                    label: preset.label,
                    value: String(preset.value),
                  }))}
                  value={String(linearAngle)}
                  variant="dark"
                  onChange={(event) =>
                    setLinearAngle(Number(event.target.value))
                  }
                />
              </div>
            ) : null}

            {type === "radial" ? (
              <div className="space-y-2 md:col-span-2">
                <Label htmlFor="radial-shape">Radial Shape</Label>
                <Select
                  id="radial-shape"
                  options={RADIAL_SHAPES}
                  value={radialShape}
                  variant="dark"
                  onChange={(event) => setRadialShape(event.target.value)}
                />
              </div>
            ) : null}

            {type === "conic" ? (
              <div className="space-y-2">
                <div className="flex items-center justify-between gap-3">
                  <Label htmlFor="conic-angle">Start Angle</Label>
                  <span className="text-sm text-muted-foreground">
                    {conicFrom}deg
                  </span>
                </div>
                <Slider
                  id="conic-angle"
                  min={0}
                  max={360}
                  step={1}
                  value={conicFrom}
                  onChange={(event) => setConicFrom(Number(event.target.value))}
                />
              </div>
            ) : null}
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          {stops.map((stop, index) => (
            <div
              key={stop.id}
              draggable
              onDragStart={() => setDragIndex(index)}
              onDragOver={(event) => event.preventDefault()}
              onDrop={() => {
                if (dragIndex === null || dragIndex === index) {
                  return;
                }

                setStops((currentStops) =>
                  moveStop(currentStops, dragIndex, index),
                );
                setDragIndex(null);
              }}
              onDragEnd={() => setDragIndex(null)}
              className="rounded-2xl border bg-background/50 p-4"
            >
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <div className="rounded-xl border bg-background/80 p-2 text-muted-foreground">
                    <GripVertical className="size-4" />
                  </div>
                  <div>
                    <p className="text-sm font-medium">Stop {index + 1}</p>
                    <p className="text-xs text-muted-foreground">
                      Drag to change the stop order.
                    </p>
                  </div>
                </div>

                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  onClick={() =>
                    setStops((currentStops) =>
                      currentStops.filter(
                        (currentStop) => currentStop.id !== stop.id,
                      ),
                    )
                  }
                  disabled={stops.length <= 2}
                >
                  <Trash2 className="size-4" />
                </Button>
              </div>

              <div
                className="mt-4 grid gap-4 md:grid-cols-[auto_1fr]"
                draggable
                onDragStart={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                }}
              >
                <ColorInput
                  aria-label={`Stop ${index + 1} color`}
                  value={stop.color}
                  onChange={(event) =>
                    setStops((currentStops) =>
                      currentStops.map((currentStop) =>
                        currentStop.id === stop.id
                          ? {
                              ...currentStop,
                              color: event.target.value.toUpperCase(),
                            }
                          : currentStop,
                      ),
                    )
                  }
                  variant="dark"
                  className="h-12 w-16"
                />

                <div className="space-y-3">
                  <div className="space-y-2">
                    <Label htmlFor={`stop-color-${stop.id}`}>Color Value</Label>
                    <Input
                      id={`stop-color-${stop.id}`}
                      value={stop.color}
                      onChange={(event) =>
                        setStops((currentStops) =>
                          currentStops.map((currentStop) =>
                            currentStop.id === stop.id
                              ? { ...currentStop, color: event.target.value }
                              : currentStop,
                          ),
                        )
                      }
                      spellCheck={false}
                      variant="dark"
                    />
                  </div>

                  <div className="space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <Label htmlFor={`stop-position-${stop.id}`}>
                        Position
                      </Label>
                      <span className="text-sm text-muted-foreground">
                        {stop.position}%
                      </span>
                    </div>
                    <Slider
                      id={`stop-position-${stop.id}`}
                      min={0}
                      max={100}
                      step={1}
                      value={stop.position}
                      onChange={(event) =>
                        setStops((currentStops) =>
                          currentStops.map((currentStop) =>
                            currentStop.id === stop.id
                              ? {
                                  ...currentStop,
                                  position: Number(event.target.value),
                                }
                              : currentStop,
                          ),
                        )
                      }
                    />
                  </div>
                </div>
              </div>
            </div>
          ))}

          <Button
            type="button"
            variant="outline"
            className="w-full gap-2"
            onClick={() => setStops((currentStops) => addStop(currentStops))}
          >
            <Plus className="size-4" />
            Add Color Stop
          </Button>
        </CardContent>
      </Card>

      <div className="space-y-6">
        <Card className="border-white/10 bg-card/70">
          <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
            <div>
              <CardTitle>Preview</CardTitle>
              <p className="mt-2 text-sm text-muted-foreground">
                Live render of the gradient exactly as it will export.
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <CopyButton value={gradientCss} label="Copy Gradient" />
              <Button
                type="button"
                onClick={handleDownload}
                disabled={isDownloading}
                className="gap-2"
              >
                <Download className="size-4" />
                {isDownloading ? "Rendering..." : "Download PNG"}
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            <div
              className="min-h-[320px] rounded-[2rem] border shadow-inner"
              style={{ backgroundImage: gradientCss }}
            />
          </CardContent>
        </Card>

        <div className="space-y-4">
          <OutputField
            label="CSS Background Property"
            description="Paste directly into stylesheets or inline style objects."
            value={backgroundProperty}
          />
          <OutputField
            label="Tailwind Arbitrary Value"
            description="Ready for `className` usage in Tailwind projects."
            value={tailwindArbitrary}
          />
        </div>
      </div>
    </div>
  );
}
