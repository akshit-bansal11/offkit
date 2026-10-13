"use client";

import {
  ArrowRight,
  CheckCircle2,
  Copy,
  Download,
  FileCode2,
  Settings,
} from "lucide-react";
import React, { useMemo, useState } from "react";
import { optimize } from "svgo/browser";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Checkbox } from "@/components/ui/form/Checkbox";
import { Label } from "@/components/ui/form/Label";
import { Textarea } from "@/components/ui/form/Textarea";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { ToggleSwitch } from "@/components/ui/interaction/ToggleSwitch";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("svg-optimizer");

export default function SvgOptimizerPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <SvgOptimizerTool />
    </ToolPageShell>
  );
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

function getLineSummary(value: string) {
  if (!value.trim()) {
    return "Empty";
  }

  const lines = value.split("\n").length;
  return `Line 1, Col 1 · ${lines} lines`;
}

const KEBAB_TO_CAMEL = [
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-miterlimit",
  "fill-rule",
  "clip-rule",
  "fill-opacity",
  "stroke-opacity",
  "stroke-dasharray",
  "stroke-dashoffset",
  "font-family",
  "font-size",
  "font-weight",
  "text-anchor",
  "alignment-baseline",
  "clip-path",
  "stop-color",
  "stop-opacity",
  "vector-effect",
];

function convertToJSX(svgString: string) {
  let jsx = svgString.replace(/class=/g, "className=");
  jsx = jsx.replace(/for=/g, "htmlFor=");

  // Replace kebab-case SVG attributes with camelCase
  KEBAB_TO_CAMEL.forEach((kebab) => {
    const camel = kebab.replace(/-([a-z])/g, (g) => g[1].toUpperCase());
    const regex = new RegExp(`${kebab}=`, "gi");
    jsx = jsx.replace(regex, `${camel}=`);
  });

  // Inject props
  jsx = jsx.replace(/<svg\s*/, "<svg {...props} ");

  return `export default function SvgIcon(props) {\n  return (\n    ${jsx.trim().replace(/\n/g, "\n    ")}\n  );\n}`;
}

function SvgOptimizerTool() {
  const [inputSvg, setInputSvg] = useState<string>("");

  // Notification UI
  const [copiedId, setCopiedId] = useState<"optimized" | "jsx" | null>(null);

  // Settings
  const [precision, setPrecision] = useState<number>(3);
  const [removeComments, setRemoveComments] = useState(true);
  const [removeMetadata, setRemoveMetadata] = useState(true);
  const [removeEmptyAttrs, setRemoveEmptyAttrs] = useState(true);
  const [collapseGroups, setCollapseGroups] = useState(true);
  const [stripNamespaces, setStripNamespaces] = useState(true);
  const [convertStyles, setConvertStyles] = useState(true); // convertStyleToAttrs

  const [enableJsx, setEnableJsx] = useState(false);

  // Process File wrapper
  const handleFileLoad = (file: File) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      const result = e.target?.result as string;
      setInputSvg(result);
    };
    reader.readAsText(file);
  };

  // Compute Optimizations live
  const optimizedData = useMemo(() => {
    if (!inputSvg.trim()) {
      return {
        output: "",
        jsxOutput: "",
        preview: null,
        origSize: 0,
        optiSize: 0,
      };
    }

    try {
      const plugins: unknown[] = [];

      if (removeComments) plugins.push("removeComments");
      if (removeMetadata)
        plugins.push(
          "removeMetadata",
          "removeTitle",
          "removeDesc",
          "removeXMLProcInst",
        );
      if (removeEmptyAttrs)
        plugins.push("removeEmptyAttrs", "removeEmptyContainers");
      if (collapseGroups) plugins.push("collapseGroups");
      if (stripNamespaces) plugins.push("removeEditorsNSData");
      if (convertStyles) plugins.push("convertStyleToAttrs");

      plugins.push("cleanupIds", "minifyStyles", "removeUnknownsAndDefaults");

      const result = optimize(inputSvg, {
        multipass: true,
        plugins: [
          // @ts-expect-error - plugins array is dynamically built
          ...plugins,
          // @ts-expect-error - overriding generic SVGO preset config
          {
            name: "preset-default",
            params: {
              overrides: {
                cleanupNumericValues: { floatPrecision: precision },
                convertPathData: { floatPrecision: precision },
                transformGroupAnimations: false, // Safer defaults
              },
            },
          },
        ],
      });

      const optiSvg = result.data;
      const optiBlobSize = new Blob([optiSvg]).size;
      const origBlobSize = new Blob([inputSvg]).size;

      const computedJsx = enableJsx ? convertToJSX(optiSvg) : "";

      return {
        output: optiSvg,
        jsxOutput: computedJsx,
        preview: optiSvg,
        origSize: origBlobSize,
        optiSize: optiBlobSize,
      };
    } catch (e) {
      console.error("SVGO optimization failed:", e);
      return {
        output: "Error parsing/optimizing SVG. Ensure valid syntax.",
        jsxOutput: "",
        preview: null,
        origSize: 0,
        optiSize: 0,
      };
    }
  }, [
    inputSvg,
    precision,
    removeComments,
    removeMetadata,
    removeEmptyAttrs,
    collapseGroups,
    stripNamespaces,
    convertStyles,
    enableJsx,
  ]);

  const savingsPct =
    optimizedData.origSize > 0
      ? Math.round(
          ((optimizedData.origSize - optimizedData.optiSize) /
            optimizedData.origSize) *
            100,
        )
      : 0;

  const handleCopy = (text: string, id: "optimized" | "jsx") => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const handleDownload = () => {
    if (!optimizedData.output) return;
    const blob = new Blob([optimizedData.output], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "optimized.svg";
    a.click();
    URL.revokeObjectURL(url);
  };

  // Convert encoded data URL for visual background preview safely
  const previewDataUrl = useMemo(() => {
    if (!optimizedData.preview) return null;
    return `data:image/svg+xml;base64,${btoa(encodeURIComponent(optimizedData.preview).replace(/%([0-9A-F]{2})/g, (match, p1) => String.fromCharCode(Number("0x" + p1))))}`;
  }, [optimizedData.preview]);

  return (
    <div className="w-full space-y-6">
      <div className="grid gap-6 xl:grid-cols-[300px_1fr]">
        {/* Settings Sidebar */}
        <div className="space-y-6">
          <Card className="tool-card-inline sticky top-6 lg:top-8">
            <CardHeader className="border-b border-white/5 pb-4">
              <CardTitle className="flex items-center gap-2 text-lg">
                <Settings className="size-5 text-muted-foreground" />
                Optimization Settings
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-6 space-y-6">
              <div className="space-y-4">
                <Label className="flex items-center justify-between">
                  Decimal Precision
                  <Badge variant="outline" className="font-mono">
                    {precision}
                  </Badge>
                </Label>
                <Slider
                  min={0}
                  max={5}
                  step={1}
                  value={precision}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setPrecision(Number(e.target.value))
                  }
                />
                <p className="text-[11px] text-muted-foreground mt-1">
                  Lower numbers heavily compress paths but can distort curves.
                  Recommended: 2-3.
                </p>
              </div>

              <div className="space-y-3">
                <Label>Strip Toggles</Label>

                <Label className="flex items-center gap-3 p-1 text-sm text-foreground hover:cursor-pointer">
                  <Checkbox
                    checked={removeComments}
                    onCheckedChange={setRemoveComments}
                  />
                  Remove Comments
                </Label>

                <Label className="flex items-center gap-3 p-1 text-sm text-foreground hover:cursor-pointer">
                  <Checkbox
                    checked={removeMetadata}
                    onCheckedChange={setRemoveMetadata}
                  />
                  Remove Metadata & Title
                </Label>

                <Label className="flex items-center gap-3 p-1 text-sm text-foreground hover:cursor-pointer">
                  <Checkbox
                    checked={removeEmptyAttrs}
                    onCheckedChange={setRemoveEmptyAttrs}
                  />
                  Remove Empty Groups & Attrs
                </Label>

                <Label className="flex items-center gap-3 p-1 text-sm text-foreground hover:cursor-pointer">
                  <Checkbox
                    checked={collapseGroups}
                    onCheckedChange={setCollapseGroups}
                  />
                  Collapse Useless Groups
                </Label>

                <Label className="flex items-center gap-3 p-1 text-sm text-foreground hover:cursor-pointer">
                  <Checkbox
                    checked={stripNamespaces}
                    onCheckedChange={setStripNamespaces}
                  />
                  Strip Editor Namespaces
                </Label>

                <Label className="flex items-center gap-3 p-1 text-sm text-foreground hover:cursor-pointer">
                  <Checkbox
                    checked={convertStyles}
                    onCheckedChange={setConvertStyles}
                  />
                  Convert Inline Styles to Attrs
                </Label>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Main Content Area */}
        <div className="space-y-6">
          <div className="grid gap-6 lg:grid-cols-[1fr_200px] xl:grid-cols-[1fr_300px]">
            {/* Input Header & Dragzone */}
            <div className="space-y-4">
              <Label>Original SVG Input</Label>
              <FileDropZoneCard
                fileTypeLabel="an SVG file"
                supportedFormats="SVG"
                accept=".svg,image/svg+xml"
                onFilesSelected={(incoming) => {
                  const validSvg = incoming.find(
                    (file) =>
                      file.name.toLowerCase().endsWith(".svg") ||
                      file.type === "image/svg+xml",
                  );

                  if (validSvg) {
                    handleFileLoad(validSvg);
                  }
                }}
              >
                <div className="relative">
                  {inputSvg ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="absolute right-3 top-3 z-10 text-muted-foreground hover:text-red-400"
                      onClick={() => setInputSvg("")}
                    >
                      Clear
                    </Button>
                  ) : null}

                  <Textarea
                    value={inputSvg}
                    onChange={(e) => setInputSvg(e.target.value)}
                    spellCheck={false}
                    placeholder="Paste raw <svg> markup here, or drop a file anywhere on this box..."
                    variant="dark"
                    className="min-h-[260px] w-full resize-none border-white/10 p-4 pt-12 font-mono text-xs leading-relaxed focus-visible:ring-0"
                  />
                </div>
              </FileDropZoneCard>
            </div>

            {/* Live Visual Preview */}
            <div className="space-y-4">
              <Label>Live Render Output</Label>
              <div className="flex h-[260px] items-center justify-center overflow-hidden rounded-[1.5rem] bg-black/10 border border-white/5 shadow-inner">
                {previewDataUrl ? (
                  /* eslint-disable-next-line @next/next/no-img-element */
                  <img
                    src={previewDataUrl}
                    alt="Optimized Preview"
                    className="h-[200px] w-[200px] object-contain drop-shadow-2xl"
                  />
                ) : (
                  <div className="text-muted-foreground/30 flex flex-col items-center">
                    <FileCode2 className="size-16 mb-4 opacity-30" />
                    <span className="text-xs">Preview Empty</span>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Stats Bar */}
          <div className="flex flex-wrap items-center justify-between rounded-xl bg-card border border-white/5 p-4 shadow-sm">
            <div className="flex items-center gap-6">
              <div>
                <p className="text-[10px] uppercase text-muted-foreground font-semibold tracking-wider">
                  Original Size
                </p>
                <p className="font-mono text-sm mt-0.5">
                  {formatFileSize(optimizedData.origSize)}
                </p>
              </div>
              <ArrowRight className="size-4 text-muted-foreground/30" />
              <div>
                <p className="text-[10px] uppercase text-muted-foreground font-semibold tracking-wider">
                  Optimized Size
                </p>
                <p className="mt-0.5 font-mono text-sm text-white/80">
                  {formatFileSize(optimizedData.optiSize)}
                </p>
              </div>
            </div>
            {optimizedData.origSize > 0 && (
              <Badge
                variant="outline"
                className={`px-3 py-1 font-mono text-sm border-white/15 bg-white/[0.06] text-white/75 ${savingsPct < 0 ? "border-red-500/30 bg-red-500/10 text-red-300" : ""}`}
              >
                {savingsPct > 0
                  ? `-${savingsPct}% Saved`
                  : savingsPct === 0
                    ? "No Change"
                    : `+${Math.abs(savingsPct)}% Larger`}
              </Badge>
            )}
          </div>

          <div className="rounded-2xl border border-white/5 bg-card/90 p-3 shadow-sm">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
              <div className="flex flex-wrap items-center gap-4">
                <div className="flex items-center gap-3 rounded-xl border border-white/5 bg-background/40 px-3 py-2">
                  <Label>Convert to JSX</Label>
                  <ToggleSwitch
                    checked={enableJsx}
                    onCheckedChange={setEnableJsx}
                    size="sm"
                    variant="default"
                  />
                </div>

                <div className="hidden h-8 w-px bg-white/5 lg:block" />

                <div className="flex items-center gap-2">
                  <span className="text-[11px] font-semibold uppercase tracking-[0.24em] text-muted-foreground">
                    Output
                  </span>
                  <Badge
                    variant="outline"
                    className="border-white/10 bg-background/40 px-2.5 py-1 text-[11px] uppercase tracking-[0.18em] text-foreground"
                  >
                    SVG
                  </Badge>
                  <Badge
                    variant="outline"
                    className={
                      enableJsx
                        ? "border-primary/30 bg-primary/10 px-2.5 py-1 text-[11px] uppercase tracking-[0.18em] text-primary"
                        : "border-white/10 bg-background/40 px-2.5 py-1 text-[11px] uppercase tracking-[0.18em] text-muted-foreground"
                    }
                  >
                    JSX
                  </Badge>
                </div>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => handleCopy(optimizedData.output, "optimized")}
                  disabled={!optimizedData.output}
                >
                  {copiedId === "optimized" ? (
                    <CheckCircle2 className="mr-2 size-3.5 text-primary" />
                  ) : (
                    <Copy className="mr-2 size-3.5" />
                  )}
                  Copy Optimized
                </Button>

                {enableJsx ? (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handleCopy(optimizedData.jsxOutput, "jsx")}
                    disabled={!optimizedData.jsxOutput}
                  >
                    {copiedId === "jsx" ? (
                      <CheckCircle2 className="mr-2 size-3.5 text-primary" />
                    ) : (
                      <Copy className="mr-2 size-3.5" />
                    )}
                    Copy JSX
                  </Button>
                ) : null}

                <Button
                  size="sm"
                  variant="default"
                  onClick={handleDownload}
                  className="btn-accent"
                  disabled={!optimizedData.output}
                >
                  <Download className="mr-2 size-3.5" />
                  Download SVG
                </Button>
              </div>
            </div>
          </div>

          <div className="grid gap-6 xl:grid-cols-2">
            <Card className="overflow-hidden border-white/10 bg-card/70 shadow-sm">
              <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 bg-background/20 px-4 py-3">
                <div className="flex items-center gap-2">
                  <span className="size-2 rounded-full bg-amber-400/70" />
                  <CardTitle className="text-sm uppercase tracking-[0.18em] text-foreground/90">
                    Optimized SVG
                  </CardTitle>
                </div>
                <span className="text-[11px] font-mono text-muted-foreground">
                  {getLineSummary(optimizedData.output)}
                </span>
              </CardHeader>
              <CardContent className="p-0">
                <Textarea
                  value={optimizedData.output}
                  readOnly
                  spellCheck={false}
                  placeholder="Optimized SVG will appear here."
                  variant="dark"
                  className="min-h-[340px] resize-none border-0 p-4 font-mono text-xs leading-relaxed text-white/85 focus-visible:ring-0"
                />
              </CardContent>
            </Card>

            <Card className="overflow-hidden border-white/10 bg-card/70 shadow-sm">
              <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 bg-background/20 px-4 py-3">
                <div className="flex items-center gap-2">
                  <span className="size-2 rounded-full bg-sky-400/70" />
                  <CardTitle className="text-sm uppercase tracking-[0.18em] text-foreground/90">
                    JSX Output
                  </CardTitle>
                </div>
                <Badge
                  variant="outline"
                  className={
                    enableJsx
                      ? "border-primary/25 bg-primary/10 text-primary"
                      : "border-white/10 bg-background/40 text-muted-foreground"
                  }
                >
                  {enableJsx ? "Generated" : "Disabled"}
                </Badge>
              </CardHeader>
              <CardContent className="p-0">
                <Textarea
                  value={
                    enableJsx
                      ? optimizedData.jsxOutput
                      : "Enable Convert to JSX to generate React-friendly output."
                  }
                  readOnly
                  spellCheck={false}
                  variant="dark"
                  className="min-h-[340px] resize-none border-0 p-4 font-mono text-xs leading-relaxed text-white/85 focus-visible:ring-0"
                />
              </CardContent>
            </Card>
          </div>
        </div>
      </div>
    </div>
  );
}
