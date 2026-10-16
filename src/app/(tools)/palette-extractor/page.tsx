"use client";

import { CheckCircle2, Copy, Loader2 } from "lucide-react";
import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/design/CopyButton";
import { Badge } from "@/components/ui/feedback/Badge";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import { extractPalette } from "@/lib/design-tools/palette-extractor";

const tool = getToolBySlug("palette-extractor");

export default function PaletteExtractorPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <PaletteExtractorTool />
    </ToolPageShell>
  );
}

function PaletteExtractorTool() {
  const [image, setImage] = useState<string | null>(null);
  const [colors, setColors] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedColor, setCopiedColor] = useState<string | null>(null);

  async function handleFile(file: File | null) {
    if (!file) {
      return;
    }

    const reader = new FileReader();
    reader.onloadend = async () => {
      const result = reader.result as string;
      setImage(result);
      setColors([]);
      setError(null);

      try {
        setIsLoading(true);
        const palette = await extractPalette(file);
        setColors(palette.colors);
      } catch (paletteError) {
        setError(
          paletteError instanceof Error
            ? `Could not extract colors: ${paletteError.message}`
            : "Could not extract colors from the image.",
        );
      } finally {
        setIsLoading(false);
      }
    };
    reader.readAsDataURL(file);
  }

  async function handleCopyColor(color: string) {
    await navigator.clipboard.writeText(color);
    setCopiedColor(color);
    window.setTimeout(() => setCopiedColor(null), 1600);
  }

  const paletteJson = JSON.stringify(
    {
      name: image ? "Extracted Palette" : "Palette",
      colors,
    },
    null,
    2,
  );

  return (
    <>
      <div className="grid gap-6 xl:grid-cols-[0.95fr_1.05fr]">
        <Card className="border-white/10 bg-card/70">
          <CardHeader className="space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>Upload an Image</CardTitle>
                <p className="mt-2 text-sm text-muted-foreground">
                  The extractor samples the image on your device and returns its
                  most dominant, visibly distinct colors.
                </p>
              </div>
            </div>
          </CardHeader>

          <CardContent className="space-y-4">
            <FileDropZoneCard
              fileTypeLabel="an image"
              supportedFormats="PNG, JPG, JPEG, WEBP, and GIF"
              accept="image/*"
              disabled={isLoading}
              onFilesSelected={(files) => {
                void handleFile(files[0] ?? null);
              }}
            >
              <p className="text-sm text-muted-foreground">
                The image never leaves your device. Colors are computed locally.
              </p>
            </FileDropZoneCard>

            {image ? (
              <div className="overflow-hidden rounded-[2rem] border">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src={image}
                  alt="Uploaded preview"
                  className="w-full object-cover"
                />
              </div>
            ) : null}

            {error ? (
              <div className="rounded-2xl border border-red-500/20 bg-red-500/10 p-4 text-sm text-red-300">
                {error}
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Card className="border-white/10 bg-card/70">
          <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
            <div>
              <CardTitle>Extracted Palette</CardTitle>
              <p className="mt-2 text-sm text-muted-foreground">
                Copy individual swatches or export the full palette as JSON.
              </p>
            </div>
            {colors.length > 0 ? (
              <CopyButton value={paletteJson} label="Copy JSON" />
            ) : null}
          </CardHeader>

          <CardContent className="space-y-4">
            {isLoading ? (
              <div className="flex min-h-[320px] flex-col items-center justify-center gap-3 rounded-[2rem] border bg-background/50">
                <Loader2 className="size-8 animate-spin" />
                <p className="text-sm text-muted-foreground">
                  Extracting dominant colors...
                </p>
              </div>
            ) : null}

            {!isLoading && colors.length === 0 ? (
              <div className="flex min-h-[320px] items-center justify-center rounded-[2rem] border bg-background/50 px-6 text-center text-sm leading-6 text-muted-foreground">
                Upload an image to generate a palette of dominant colors.
              </div>
            ) : null}

            {!isLoading && colors.length > 0 ? (
              <>
                <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
                  {colors.map((color) => (
                    <Button
                      key={color}
                      type="button"
                      variant="ghost"
                      onClick={() => handleCopyColor(color)}
                      className="group h-auto justify-start rounded-[1.5rem] border bg-background/50 p-4 text-left font-normal transition hover:border-primary/40 hover:bg-background/50"
                    >
                      <div
                        className="h-28 rounded-2xl border"
                        style={{ backgroundColor: color }}
                      />
                      <div className="mt-4 flex items-center justify-between gap-3">
                        <div>
                          <p className="text-sm font-medium">{color}</p>
                          <p className="text-xs text-muted-foreground">
                            Click to copy
                          </p>
                        </div>
                        {copiedColor === color ? (
                          <Badge className="gap-1.5 bg-white/[0.06] text-white/75">
                            <CheckCircle2 className="size-3.5" />
                            Copied
                          </Badge>
                        ) : (
                          <Badge variant="outline" className="gap-1.5">
                            <Copy className="size-3.5" />
                            Copy
                          </Badge>
                        )}
                      </div>
                    </Button>
                  ))}
                </div>

                <div className="rounded-2xl border bg-background/60 p-4">
                  <p className="text-sm font-medium">Palette JSON</p>
                  <pre className="mt-3 overflow-x-auto whitespace-pre-wrap break-all rounded-xl border bg-background/80 p-4 font-mono text-sm text-white/80">
                    {paletteJson}
                  </pre>
                </div>
              </>
            ) : null}
          </CardContent>
        </Card>
      </div>
    </>
  );
}
