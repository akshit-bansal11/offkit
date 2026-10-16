"use client";

import {
  CheckCircle2,
  Copy,
  Droplet,
  Image as ImageIcon,
  SlidersHorizontal,
} from "lucide-react";
import React, { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
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

const tool = getToolBySlug("glassmorphism");

export default function GlassmorphismPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <GlassmorphismTool />
    </ToolPageShell>
  );
}

function GlassmorphismTool() {
  const [blur, setBlur] = useState<number>(10);
  const [opacity, setOpacity] = useState<number>(15);
  const [color, setColor] = useState<string>("#ffffff");
  const [borderOpacity, setBorderOpacity] = useState<number>(20);
  const [saturation, setSaturation] = useState<number>(150);

  const [bgType, setBgType] = useState<"abstract" | "gradient" | "solid">(
    "abstract",
  );
  const [copiedCSS, setCopiedCSS] = useState(false);
  const [copiedTW, setCopiedTW] = useState(false);

  function hexToRgba(hex: string, alpha: number) {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return `rgba(${r}, ${g}, ${b}, ${alpha / 100})`;
  }

  const cssValue = useMemo(() => {
    return [
      `background: ${hexToRgba(color, opacity)};`,
      `backdrop-filter: blur(${blur}px) saturate(${saturation}%);`,
      `-webkit-backdrop-filter: blur(${blur}px) saturate(${saturation}%);`,
      `border: 1px solid ${hexToRgba("#ffffff", borderOpacity)};`,
      `border-radius: 1rem;`,
    ].join("\n");
  }, [blur, opacity, color, borderOpacity, saturation]);

  // Provide an approximate Tailwind output bridging arbitrary values where necessary
  const tailwindValue = useMemo(() => {
    const rgb = color.replace("#", "");
    const r = parseInt(rgb.slice(0, 2), 16);
    const g = parseInt(rgb.slice(2, 4), 16);
    const b = parseInt(rgb.slice(4, 6), 16);

    // Map blur roughly to TS classes
    let blurClass = `backdrop-blur-[${blur}px]`;
    if (blur === 0) blurClass = "backdrop-blur-none";
    if (blur === 4) blurClass = "backdrop-blur-sm";
    if (blur === 8) blurClass = "backdrop-blur-md";
    if (blur === 12) blurClass = "backdrop-blur-lg";
    if (blur === 16) blurClass = "backdrop-blur-xl";
    if (blur === 24) blurClass = "backdrop-blur-2xl";
    if (blur === 40) blurClass = "backdrop-blur-3xl";

    return `bg-[rgba(${r},${g},${b},${opacity / 100})] ${blurClass} backdrop-saturate-[${saturation}%] border border-white/${borderOpacity} rounded-2xl`;
  }, [blur, opacity, color, borderOpacity, saturation]);

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

  const getBackgroundStyle = () => {
    if (bgType === "abstract") {
      return {
        backgroundImage:
          "radial-gradient(circle at 10% 20%, rgb(180, 50, 150) 0%, transparent 40%), radial-gradient(circle at 90% 80%, rgb(50, 150, 250) 0%, transparent 40%), radial-gradient(circle at 50% 50%, rgb(50, 200, 150) 0%, transparent 60%)",
        backgroundColor: "#0f172a",
      };
    } else if (bgType === "gradient") {
      return {
        background: "linear-gradient(45deg, #f43f5e, #8b5cf6, #3b82f6)",
      };
    } else {
      return {
        backgroundColor: "#1e293b",
      };
    }
  };

  return (
    <div className="space-y-6 max-w-5xl mx-auto xl:max-w-7xl">
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-[2fr_1fr]">
        {/* PREVIEW DOMAIN */}
        <div className="space-y-6 flex flex-col">
          <Card className="border-white/10 bg-card/70 overflow-hidden flex-1 shadow-sm flex flex-col rounded-3xl">
            <CardHeader className="bg-background/20 border-b border-white/5 py-3 px-4 flex flex-row flex-wrap items-center justify-between gap-2">
              <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                <ImageIcon className="size-4" /> Live Environment
              </CardTitle>
              <SegmentedControl
                size="sm"
                value={bgType}
                onValueChange={(value) =>
                  setBgType(value as "abstract" | "gradient" | "solid")
                }
                options={[
                  { label: "Abstract", value: "abstract" },
                  { label: "Gradient", value: "gradient" },
                  { label: "Solid", value: "solid" },
                ]}
              />
            </CardHeader>
            <CardContent
              className="flex-1 flex items-center justify-center p-8 sm:p-16 min-h-[500px] relative transition-all duration-[3000ms] overflow-hidden"
              style={getBackgroundStyle()}
            >
              {/* Contextual floating elements to showcase refraction */}
              {bgType === "abstract" && (
                <>
                  <div
                    className="absolute top-[20%] left-[20%] w-32 h-32 bg-rose-500 rounded-full mix-blend-screen mix-blend-overlay filter blur-[4px] animate-pulse"
                    style={{ animationDuration: "4s" }}
                  />
                  <div
                    className="absolute bottom-[20%] right-[20%] w-48 h-48 bg-blue-500 rounded-full mix-blend-screen mix-blend-overlay filter blur-[8px] animate-pulse"
                    style={{ animationDuration: "6s" }}
                  />
                </>
              )}

              <div
                className="relative z-10 w-full max-w-sm sm:max-w-md aspect-[4/3] flex flex-col items-center justify-center p-8 transition-all duration-300 shadow-2xl"
                style={{
                  background: hexToRgba(color, opacity),
                  backdropFilter: `blur(${blur}px) saturate(${saturation}%)`,
                  WebkitBackdropFilter: `blur(${blur}px) saturate(${saturation}%)`,
                  border: `1px solid ${hexToRgba("#ffffff", borderOpacity)}`,
                  borderRadius: "1rem",
                }}
              >
                <div className="w-16 h-16 rounded-2xl bg-white/20 flex items-center justify-center mb-6 shadow-sm border border-white/20">
                  <Droplet className="size-8 text-white" />
                </div>
                <h2 className="text-white font-semibold text-2xl tracking-tight mb-2 text-center drop-shadow-md">
                  Glassmorphism
                </h2>
                <p className="text-white/80 text-center text-sm font-medium leading-relaxed drop-shadow-md">
                  Beautiful frosted visual hierarchies that securely refract
                  light directly on your device.
                </p>
              </div>
            </CardContent>
          </Card>

          {/* EXPORT CARD */}
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <Card className="border-white/10 bg-card/70 shadow-sm">
              <CardHeader className="bg-background/20 border-b border-white/5 py-3 px-4 flex flex-row flex-wrap items-center justify-between gap-2">
                <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                  Native CSS
                </CardTitle>
                <Button
                  variant={copiedCSS ? "default" : "secondary"}
                  size="sm"
                  onClick={setCssCopy}
                  className={`h-7 text-xs px-3 transition-colors ${copiedCSS ? "bg-white/20 hover:bg-white/25 text-white" : ""}`}
                >
                  {copiedCSS ? (
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
                  value={cssValue}
                  className="h-[120px] font-mono text-sm leading-relaxed p-4 bg-background/50 border-0 focus-visible:ring-0 resize-none text-blue-400"
                />
              </div>
            </Card>

            <Card className="border-white/10 bg-card/70 shadow-sm">
              <CardHeader className="bg-background/20 border-b border-white/5 py-3 px-4 flex flex-row flex-wrap items-center justify-between gap-2">
                <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                  Tailwind Classes
                </CardTitle>
                <Button
                  variant={copiedTW ? "default" : "secondary"}
                  size="sm"
                  onClick={setTwCopy}
                  className={`h-7 text-xs px-3 transition-colors ${copiedTW ? "bg-white/20 hover:bg-white/25 text-white" : ""}`}
                >
                  {copiedTW ? (
                    <>
                      <CheckCircle2 className="size-3.5 mr-2" /> Copied
                    </>
                  ) : (
                    <>
                      <Copy className="size-3.5 mr-2" /> Copy Classes
                    </>
                  )}
                </Button>
              </CardHeader>
              <div className="p-0">
                <Textarea
                  readOnly
                  value={tailwindValue}
                  className="h-[120px] font-mono text-sm leading-relaxed p-4 bg-background/50 border-0 focus-visible:ring-0 resize-none text-white/80"
                />
              </div>
            </Card>
          </div>
        </div>

        {/* CONTROLS */}
        <div className="space-y-6">
          <Card className="border-white/10 bg-card/70 shadow-sm relative overflow-hidden h-full">
            <CardHeader className="bg-background/20 border-b border-white/5 py-4 sticky top-0 z-10 backdrop-blur">
              <CardTitle className="flex items-center gap-2 text-base">
                <SlidersHorizontal className="size-4 text-primary" />
                Refraction Properties
              </CardTitle>
            </CardHeader>
            <div className="p-6 space-y-8 layer-editor">
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label className="text-foreground">Blur Radius</Label>
                  <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                    {blur}px
                  </span>
                </div>
                <Slider
                  min={0}
                  max={40}
                  step={1}
                  value={blur}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setBlur(Number(e.target.value))
                  }
                />
              </div>

              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label className="text-foreground">Tint Opacity</Label>
                  <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                    {opacity}%
                  </span>
                </div>
                <Slider
                  min={0}
                  max={100}
                  step={1}
                  value={opacity}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setOpacity(Number(e.target.value))
                  }
                />
              </div>

              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label className="text-foreground">Saturation</Label>
                  <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                    {saturation}%
                  </span>
                </div>
                <Slider
                  min={0}
                  max={200}
                  step={10}
                  value={saturation}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setSaturation(Number(e.target.value))
                  }
                />
              </div>

              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label className="text-foreground">Border Edge Opacity</Label>
                  <span className="font-mono text-xs text-muted-foreground bg-background/50 px-2 py-0.5 rounded">
                    {borderOpacity}%
                  </span>
                </div>
                <Slider
                  min={0}
                  max={100}
                  step={1}
                  value={borderOpacity}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setBorderOpacity(Number(e.target.value))
                  }
                />
              </div>

              <div className="pt-6 border-t border-white/5 flex items-center justify-between">
                <Label className="block text-foreground">Tint Color</Label>
                <div className="flex items-center gap-3">
                  <span className="font-mono text-xs text-muted-foreground uppercase">
                    {color}
                  </span>
                  <ColorInput
                    value={color}
                    onChange={(e) => setColor(e.target.value)}
                    className="block h-8 w-8 rounded-lg border-0 bg-transparent p-0"
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
