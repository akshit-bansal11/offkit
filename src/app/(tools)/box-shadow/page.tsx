"use client";

import {
  CheckCircle2,
  Copy,
  Layers,
  Plus,
  Settings2,
  Trash2,
} from "lucide-react";
import React, { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
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

const tool = getToolBySlug("box-shadow");

export default function BoxShadowPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <BoxShadowTool />
    </ToolPageShell>
  );
}

interface ShadowLayer {
  id: string;
  xOffset: number;
  yOffset: number;
  blur: number;
  spread: number;
  opacity: number;
  color: string;
  inset: boolean;
}

const defaultLayer: Omit<ShadowLayer, "id"> = {
  xOffset: 0,
  yOffset: 12,
  blur: 24,
  spread: -4,
  opacity: 25,
  color: "#000000",
  inset: false,
};

function generateId() {
  return Math.random().toString(36).substring(2, 9);
}

function hexToRgba(hex: string, opacity: number) {
  hex = hex.replace("#", "");
  if (hex.length === 3) {
    hex = hex
      .split("")
      .map((c) => c + c)
      .join("");
  }
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${opacity / 100})`;
}

function BoxShadowTool() {
  const [layers, setLayers] = useState<ShadowLayer[]>([
    { id: generateId(), ...defaultLayer },
    {
      id: generateId(),
      xOffset: 0,
      yOffset: 4,
      blur: 8,
      spread: -2,
      opacity: 10,
      color: "#000000",
      inset: false,
    },
  ]);
  const [activeLayerId, setActiveLayerId] = useState<string>(layers[0].id);
  const [boxColor, setBoxColor] = useState("#3b82f6");
  const [bgColor, setBgColor] = useState("#f1f5f9");
  const [copied, setCopied] = useState(false);

  const activeLayerIndex = useMemo(() => {
    return layers.findIndex((l) => l.id === activeLayerId);
  }, [layers, activeLayerId]);

  const activeLayer = layers[activeLayerIndex];

  const updateActiveLayer = (updates: Partial<ShadowLayer>) => {
    setLayers((prev) => {
      const arr = [...prev];
      arr[activeLayerIndex] = { ...arr[activeLayerIndex], ...updates };
      return arr;
    });
  };

  const addLayer = () => {
    const newId = generateId();
    setLayers((prev) => [{ id: newId, ...defaultLayer }, ...prev]);
    setActiveLayerId(newId);
  };

  const removeLayer = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (layers.length <= 1) return;
    setLayers((prev) => prev.filter((l) => l.id !== id));
    if (activeLayerId === id) {
      setActiveLayerId(layers.find((l) => l.id !== id)?.id || layers[0].id);
    }
  };

  const cssString = useMemo(() => {
    const shadowString = layers
      .map((layer) => {
        const inset = layer.inset ? "inset " : "";
        const color = hexToRgba(layer.color, layer.opacity);
        return `${inset}${layer.xOffset}px ${layer.yOffset}px ${layer.blur}px ${layer.spread}px ${color}`;
      })
      .join(",\n  ");
    return `box-shadow: ${shadowString};`;
  }, [layers]);

  const handleCopy = () => {
    navigator.clipboard.writeText(cssString);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="space-y-6 max-w-5xl mx-auto xl:max-w-7xl">
      <div className="grid gap-6 lg:grid-cols-[3fr_2fr]">
        {/* Preview Area (Left Side) */}
        <div className="space-y-6 flex flex-col">
          <Card className="border-white/10 bg-card/70 overflow-hidden flex-1 shadow-sm flex flex-col">
            <CardHeader className="bg-background/20 border-b border-white/5 py-3 px-4">
              <CardTitle className="text-sm font-medium text-muted-foreground flex items-center justify-between">
                Live Preview
                <div className="flex gap-2 items-center">
                  <div className="h-4 w-px bg-white/10 mx-2" />
                  <Label className="group flex items-center gap-1.5 text-xs">
                    <span className="text-muted-foreground">Box:</span>
                    <ColorInput
                      value={boxColor}
                      onChange={(e) => setBoxColor(e.target.value)}
                      className="block h-5 w-5 rounded-md border-0 bg-transparent p-0"
                    />
                  </Label>
                  <Label className="group ml-3 flex items-center gap-1.5 text-xs">
                    <span className="text-muted-foreground">Bg:</span>
                    <ColorInput
                      value={bgColor}
                      onChange={(e) => setBgColor(e.target.value)}
                      className="block h-5 w-5 rounded-md border-0 bg-transparent p-0"
                    />
                  </Label>
                </div>
              </CardTitle>
            </CardHeader>
            <CardContent
              className="flex-1 flex items-center justify-center p-12 min-h-[400px] transition-colors relative dotted-bg"
              style={{ backgroundColor: bgColor }}
            >
              {/* Optional dot overlay for depth estimation */}
              <div
                className="absolute inset-0 pointer-events-none"
                style={{
                  backgroundImage:
                    "radial-gradient(rgba(0,0,0,0.05) 1px, transparent 1px)",
                  backgroundSize: "24px 24px",
                }}
              ></div>

              <div
                className="w-48 h-48 sm:w-64 sm:h-64 rounded-3xl transition-transform hover:scale-105 duration-300 relative z-10 border border-white/10"
                style={{
                  backgroundColor: boxColor,
                  boxShadow: layers
                    .map(
                      (layer) =>
                        `${layer.inset ? "inset " : ""}${layer.xOffset}px ${layer.yOffset}px ${layer.blur}px ${layer.spread}px ${hexToRgba(layer.color, layer.opacity)}`,
                    )
                    .join(", "),
                }}
              />
            </CardContent>
          </Card>

          {/* CSS Output Panel */}
          <Card className="border-white/10 bg-card/70 shadow-sm shrink-0">
            <CardHeader className="bg-background/20 border-b border-white/5 py-3 px-4 flex flex-row items-center justify-between">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                CSS Output
              </CardTitle>
              <Button
                variant={copied ? "default" : "secondary"}
                size="sm"
                onClick={handleCopy}
                className={`h-7 text-xs px-3 transition-colors ${copied ? "bg-white/20 hover:bg-white/25 text-white" : ""}`}
              >
                {copied ? (
                  <>
                    <CheckCircle2 className="size-3.5 mr-2" /> Copied
                  </>
                ) : (
                  <>
                    <Copy className="size-3.5 mr-2" /> Copy CSS
                  </>
                )}
              </Button>
            </CardHeader>
            <div className="p-0">
              <Textarea
                readOnly
                value={cssString}
                className="min-h-[100px] font-mono text-sm leading-relaxed p-4 bg-background/50 border-0 focus-visible:ring-0 resize-none"
              />
            </div>
          </Card>
        </div>

        {/* Controls Area (Right Side) */}
        <div className="space-y-6">
          <Card className="border-white/10 bg-card/70 shadow-sm relative overflow-hidden">
            <CardHeader className="bg-background/20 border-b border-white/5 py-4 flex flex-row items-center justify-between sticky top-0 z-10 backdrop-blur">
              <CardTitle className="flex items-center gap-2 text-base">
                <Layers className="size-4 text-primary" />
                Shadow Layers
              </CardTitle>
              <Button
                size="sm"
                variant="outline"
                onClick={addLayer}
                className="h-8 gap-1.5 text-xs bg-background/50"
              >
                <Plus className="size-3.5" /> Add Shadow
              </Button>
            </CardHeader>
            <div className="p-4 space-y-4 max-h-[600px] overflow-y-auto custom-scrollbar">
              {/* Layer Selection List */}
              <div className="space-y-2">
                {layers.map((layer, index) => (
                  <div
                    key={layer.id}
                    className={`flex items-center justify-between gap-2 rounded-xl border px-3 py-2.5 transition-all ${
                      activeLayerId === layer.id
                        ? "bg-primary/10 border-primary/50 ring-1 ring-primary/20"
                        : "bg-background/40 border-border hover:bg-background/80"
                    }`}
                  >
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => setActiveLayerId(layer.id)}
                      className="h-auto flex-1 justify-start gap-3 p-0 text-left font-normal hover:bg-transparent"
                    >
                      <div className="relative flex h-5 w-5 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-background">
                        <div
                          className="absolute inset-0"
                          style={{
                            backgroundColor: layer.color,
                            opacity: layer.opacity / 100,
                          }}
                        />
                      </div>
                      <span
                        className={`text-sm font-medium ${activeLayerId === layer.id ? "text-primary" : "text-muted-foreground"}`}
                      >
                        Layer {layers.length - index}
                        {layer.inset && (
                          <Badge
                            variant="secondary"
                            className="ml-2 h-4 select-none px-1 py-0 text-[10px] leading-none"
                          >
                            Inset
                          </Badge>
                        )}
                      </span>
                    </Button>
                    {layers.length > 1 && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        onClick={(e) => removeLayer(layer.id, e)}
                        className="h-8 w-8 rounded-md p-0 text-muted-foreground transition-colors hover:bg-red-400/10 hover:text-red-400"
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    )}
                  </div>
                ))}
              </div>

              {/* Active Layer Editor */}
              {activeLayer && (
                <div className="pt-4 mt-2 border-t border-white/5 space-y-6 animate-in fade-in duration-300">
                  <div className="flex items-center gap-2 mb-2">
                    <Settings2 className="size-4 text-muted-foreground" />
                    <h4 className="text-sm font-medium">Layer Properties</h4>
                  </div>

                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <Label className="text-foreground">X Offset</Label>
                      <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                        {activeLayer.xOffset}px
                      </span>
                    </div>
                    <Slider
                      min={-100}
                      max={100}
                      step={1}
                      value={activeLayer.xOffset}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        updateActiveLayer({ xOffset: Number(e.target.value) })
                      }
                    />
                  </div>

                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <Label className="text-foreground">Y Offset</Label>
                      <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                        {activeLayer.yOffset}px
                      </span>
                    </div>
                    <Slider
                      min={-100}
                      max={100}
                      step={1}
                      value={activeLayer.yOffset}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        updateActiveLayer({ yOffset: Number(e.target.value) })
                      }
                    />
                  </div>

                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <Label className="text-foreground">Blur Content</Label>
                      <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                        {activeLayer.blur}px
                      </span>
                    </div>
                    <Slider
                      min={0}
                      max={150}
                      step={1}
                      value={activeLayer.blur}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        updateActiveLayer({ blur: Number(e.target.value) })
                      }
                    />
                  </div>

                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <Label className="text-foreground">Spread Radius</Label>
                      <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                        {activeLayer.spread}px
                      </span>
                    </div>
                    <Slider
                      min={-50}
                      max={100}
                      step={1}
                      value={activeLayer.spread}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        updateActiveLayer({ spread: Number(e.target.value) })
                      }
                    />
                  </div>

                  <div className="space-y-4 pt-2">
                    <div className="flex items-center justify-between">
                      <Label className="text-foreground">Opacity</Label>
                      <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                        {activeLayer.opacity}%
                      </span>
                    </div>
                    <Slider
                      min={0}
                      max={100}
                      step={1}
                      value={activeLayer.opacity}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        updateActiveLayer({ opacity: Number(e.target.value) })
                      }
                    />
                  </div>

                  <div className="flex items-center justify-between gap-4 pt-4 border-t border-white/5">
                    <div className="space-y-1">
                      <Label className="block text-foreground">
                        Shadow Color
                      </Label>
                      <div className="flex items-center gap-2">
                        <ColorInput
                          value={activeLayer.color}
                          onChange={(e) =>
                            updateActiveLayer({ color: e.target.value })
                          }
                          className="block h-8 w-8 rounded-lg border-0 bg-transparent p-0"
                        />
                        <span className="font-mono text-xs text-muted-foreground uppercase">
                          {activeLayer.color}
                        </span>
                      </div>
                    </div>

                    <div className="space-y-1 text-right">
                      <Label className="block text-foreground">Position</Label>
                      <SegmentedControl
                        size="sm"
                        value={activeLayer.inset ? "inset" : "outset"}
                        onValueChange={(value) =>
                          updateActiveLayer({ inset: value === "inset" })
                        }
                        options={[
                          { label: "Outset", value: "outset" },
                          { label: "Inset", value: "inset" },
                        ]}
                      />
                    </div>
                  </div>
                </div>
              )}
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}
