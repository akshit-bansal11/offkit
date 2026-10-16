"use client";

import { Download, File, ImageIcon, Loader2, Minimize2, X } from "lucide-react";
import React, { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { NotSmallerNotice } from "@/components/tools/pdf/NotSmallerNotice";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";
import { compressPdf } from "@/lib/pdf/compress";
import { pdfjsLib } from "@/lib/pdf/pdfjs";
import { formatFileSize } from "@/lib/tools/pdf-toolkit/utils";

const tool = getToolBySlug("pdf-compressor");

export default function PdfCompressorPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <PdfCompressorTool />
    </ToolPageShell>
  );
}

function PdfCompressorTool() {
  const [file, setFile] = useState<File | null>(null);
  const [pageCount, setPageCount] = useState(0);
  const [renderScale, setRenderScale] = useState(1.5);
  const [jpegQuality, setJpegQuality] = useState(70);
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [outputSize, setOutputSize] = useState<number | null>(null);
  const [outputUrl, setOutputUrl] = useState<string | null>(null);
  const [notSmaller, setNotSmaller] = useState(false);

  const handleFile = async (f: File) => {
    setFile(f);
    setError(null);
    setNotSmaller(false);
    setOutputSize(null);
    setOutputUrl(null);
    try {
      const buf = await f.arrayBuffer();
      const pdf = await pdfjsLib.getDocument({ data: buf }).promise;
      setPageCount(pdf.numPages);
    } catch {
      setPageCount(0);
    }
  };

  const compress = async () => {
    if (!file) return;
    setIsProcessing(true);
    setProgress(0);
    setError(null);
    setNotSmaller(false);
    if (outputUrl) URL.revokeObjectURL(outputUrl);
    setOutputUrl(null);
    setOutputSize(null);

    try {
      const blob = await compressPdf(file, {
        scale: renderScale,
        quality: jpegQuality,
        onProgress: (fraction) => setProgress(Math.round(fraction * 90)),
      });
      if (blob) {
        setOutputSize(blob.size);
        setOutputUrl(URL.createObjectURL(blob));
      } else {
        setNotSmaller(true);
      }
      setProgress(100);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Compression failed.");
    }
    setIsProcessing(false);
  };

  const download = () => {
    if (!outputUrl) return;
    const a = document.createElement("a");
    a.href = outputUrl;
    a.download = `compressed_${file?.name ?? "document.pdf"}`;
    a.click();
  };

  const reduction =
    file && outputSize ? Math.round((1 - outputSize / file.size) * 100) : null;

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="a PDF file"
        supportedFormats="PDF"
        accept="application/pdf"
        multiple={false}
        onFilesSelected={(files) => {
          const f = files[0];
          if (f) void handleFile(f);
        }}
      />

      {file && (
        <div className="flex items-center gap-4 rounded-2xl border border-white/10 bg-background/45 p-4">
          <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-400">
            <File className="size-5" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">{file.name}</p>
            <p className="text-xs text-muted-foreground">
              {formatFileSize(file.size)}
              {pageCount > 0 && ` · ${pageCount} pages`}
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="size-8 text-muted-foreground"
            onClick={() => {
              setFile(null);
              setPageCount(0);
              setOutputSize(null);
              setOutputUrl(null);
              setNotSmaller(false);
            }}
          >
            <X className="size-4" />
          </Button>
        </div>
      )}

      {file && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="space-y-5 p-5">
            {/* Warning banner */}
            <div className="flex items-start gap-3 rounded-xl bg-amber-500/10 border border-amber-500/20 p-3 text-amber-400">
              <ImageIcon className="size-4 mt-0.5 shrink-0" />
              <p className="text-xs leading-relaxed">
                <strong>Note:</strong> Compression rasterizes pages into
                optimized JPEGs. Text will no longer be selectable in the output
                PDF.
              </p>
            </div>

            {/* Render scale slider */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <p className="text-sm font-medium">Rendering Scale</p>
                <Badge
                  variant="outline"
                  className="border-white/10 bg-white/[0.04] font-mono text-xs"
                >
                  {renderScale.toFixed(1)}×
                </Badge>
              </div>
              <Slider
                min={0.5}
                max={3.0}
                step={0.1}
                value={renderScale}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                  setRenderScale(Number(e.target.value))
                }
              />
              <p className="text-xs text-muted-foreground">
                Lower = smaller file, Higher = better quality
              </p>
            </div>

            {/* JPEG quality slider */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <p className="text-sm font-medium">JPEG Quality</p>
                <Badge
                  variant="outline"
                  className="border-white/10 bg-white/[0.04] font-mono text-xs"
                >
                  {jpegQuality}%
                </Badge>
              </div>
              <Slider
                min={10}
                max={100}
                step={5}
                value={jpegQuality}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                  setJpegQuality(Number(e.target.value))
                }
              />
              <p className="text-xs text-muted-foreground">
                Lower = smaller, Higher = better visual quality
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-3 px-5 py-3">
            {isProcessing && (
              <div className="flex-1 space-y-1">
                <Progress value={progress} className="h-1.5 bg-white/10" />
                <p className="text-xs text-muted-foreground">
                  Compressing… {progress}%
                </p>
              </div>
            )}

            {outputUrl && outputSize !== null && !isProcessing && (
              <div className="flex-1 space-y-1">
                <div className="flex items-center gap-3 text-xs text-muted-foreground">
                  <span>Original: {formatFileSize(file.size)}</span>
                  <span>→</span>
                  <span className="text-emerald-400 font-medium">
                    Compressed: {formatFileSize(outputSize)}
                  </span>
                  {reduction !== null && reduction > 0 && (
                    <Badge
                      variant="outline"
                      className="border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0 text-[10px] text-emerald-400"
                    >
                      -{reduction}%
                    </Badge>
                  )}
                </div>
              </div>
            )}

            {notSmaller && !isProcessing && (
              <NotSmallerNotice className="flex-1" />
            )}

            <div className="ml-auto flex items-center gap-2">
              {outputUrl && (
                <Button
                  variant="outline"
                  size="sm"
                  className="h-8 border-white/10 bg-white/[0.04] text-xs"
                  onClick={download}
                >
                  <Download className="size-3.5" />
                  Download
                </Button>
              )}
              <Button
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={isProcessing}
                onClick={() => void compress()}
              >
                {isProcessing ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Minimize2 className="size-3.5" />
                )}
                {isProcessing ? "Compressing…" : "Compress PDF"}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {!file && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <Minimize2 className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Upload a PDF to compress.
          </p>
        </Card>
      )}

      {error && (
        <div
          role="alert"
          className="rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-sm text-red-200"
        >
          {error}
        </div>
      )}
    </div>
  );
}
