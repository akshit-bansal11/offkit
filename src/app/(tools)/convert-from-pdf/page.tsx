"use client";

import JSZip from "jszip";
import { Download, File, FileInput, Loader2, X } from "lucide-react";
import React, { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";
import { pdfjsLib } from "@/lib/pdf/pdfjs";

const tool = getToolBySlug("convert-from-pdf");

export default function ConvertFromPdfPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ConvertFromPdfTool />
    </ToolPageShell>
  );
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface ExtractedPage {
  index: number;
  url: string;
  blob: Blob;
}

function ConvertFromPdfTool() {
  const [file, setFile] = useState<File | null>(null);
  const [pageCount, setPageCount] = useState(0);
  const [renderScale, setRenderScale] = useState(2.0);
  const [outputFormat, setOutputFormat] = useState<"png" | "jpeg">("png");
  const [jpegQuality, setJpegQuality] = useState(85);
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [pages, setPages] = useState<ExtractedPage[]>([]);

  const handleFile = async (f: File) => {
    setFile(f);
    setError(null);
    setPages([]);
    setProgress(0);
    try {
      const buf = await f.arrayBuffer();
      const pdf = await pdfjsLib.getDocument({ data: buf }).promise;
      setPageCount(pdf.numPages);
    } catch {
      setPageCount(0);
    }
  };

  const extract = async () => {
    if (!file) return;
    setIsProcessing(true);
    setProgress(0);
    setError(null);

    // revoke old urls
    pages.forEach((p) => URL.revokeObjectURL(p.url));
    setPages([]);

    try {
      const buf = await file.arrayBuffer();
      const pdf = await pdfjsLib.getDocument({ data: buf }).promise;
      const total = pdf.numPages;
      const extracted: ExtractedPage[] = [];

      for (let i = 1; i <= total; i++) {
        const page = await pdf.getPage(i);
        const vp = page.getViewport({ scale: renderScale });
        const canvas = document.createElement("canvas");
        const ctx = canvas.getContext("2d")!;
        canvas.width = vp.width;
        canvas.height = vp.height;
        // @ts-expect-error — pdfjs typings
        await page.render({ canvasContext: ctx, viewport: vp }).promise;
        const blob = await new Promise<Blob>((res) =>
          canvas.toBlob(
            (b) => res(b!),
            outputFormat === "png" ? "image/png" : "image/jpeg",
            jpegQuality / 100,
          ),
        );
        const url = URL.createObjectURL(blob);
        extracted.push({ index: i, url, blob });
        setPages([...extracted]);
        setProgress(Math.round((i / total) * 90));
      }
      setProgress(100);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Extraction failed.");
    }
    setIsProcessing(false);
  };

  const downloadZip = async () => {
    if (pages.length === 0) return;
    const zip = new JSZip();
    pages.forEach((p) =>
      zip.file(
        `page_${String(p.index).padStart(3, "0")}.${outputFormat}`,
        p.blob,
      ),
    );
    const blob = await zip.generateAsync({ type: "blob" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${file?.name.replace(".pdf", "") ?? "pages"}_images.zip`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const downloadSingle = (p: ExtractedPage) => {
    const a = document.createElement("a");
    a.href = p.url;
    a.download = `page_${String(p.index).padStart(3, "0")}.${outputFormat}`;
    a.click();
  };

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
          <div className="rounded-lg bg-success/10 p-2 text-success">
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
              setPages([]);
            }}
          >
            <X className="size-4" />
          </Button>
        </div>
      )}

      {file && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="grid gap-5 p-5 sm:grid-cols-3">
            {/* Render scale */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <p className="text-sm font-medium">Render Scale</p>
                <Badge
                  variant="outline"
                  className="border-white/10 bg-white/[0.04] font-mono text-xs"
                >
                  {renderScale.toFixed(1)}×
                </Badge>
              </div>
              <Slider
                min={1.0}
                max={3.0}
                step={0.5}
                value={renderScale}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                  setRenderScale(Number(e.target.value))
                }
              />
            </div>

            {/* Output format */}
            <div className="space-y-3">
              <p className="text-sm font-medium">Output Format</p>
              <Select
                options={[
                  { label: "PNG (lossless)", value: "png" },
                  { label: "JPEG (smaller)", value: "jpeg" },
                ]}
                value={outputFormat}
                onChange={(e) =>
                  setOutputFormat(e.target.value as "png" | "jpeg")
                }
                className="h-9 border-white/10 bg-white/[0.04] text-sm"
              />
            </div>

            {/* JPEG quality (only when JPEG) */}
            {outputFormat === "jpeg" && (
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
              </div>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-3 px-5 py-3">
            {isProcessing && (
              <div className="flex-1">
                <Progress value={progress} className="h-1.5 bg-white/10" />
              </div>
            )}

            {!isProcessing && pages.length > 0 && (
              <p className="text-xs text-muted-foreground flex-1">
                {pages.length} images extracted
              </p>
            )}

            <div className="ml-auto flex items-center gap-2">
              {pages.length > 0 && !isProcessing && (
                <Button
                  variant="outline"
                  size="sm"
                  className="h-8 border-white/10 bg-white/[0.04] text-xs"
                  onClick={() => void downloadZip()}
                >
                  <Download className="size-3.5" />
                  Download ZIP
                </Button>
              )}
              <Button
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={isProcessing}
                onClick={() => void extract()}
              >
                {isProcessing ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <FileInput className="size-3.5" />
                )}
                {isProcessing ? "Extracting…" : "Extract Images"}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {pages.length > 0 && (
        <Card className="border-white/10 bg-white/[0.015] p-5">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 md:grid-cols-6 lg:grid-cols-8">
            {pages.map((p) => (
              <div
                key={p.index}
                className="group relative overflow-hidden rounded-xl border border-white/10 bg-background/30 aspect-[1/1.41]"
              >
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src={p.url}
                  alt={`Page ${p.index}`}
                  className="h-full w-full object-contain bg-white"
                />
                <div className="absolute inset-x-0 bottom-0 flex items-center justify-between bg-black/70 px-2 py-1">
                  <span className="text-[9px] text-white">{p.index}</span>
                  <button
                    onClick={() => downloadSingle(p)}
                    className="text-white/60 hover:text-white transition-colors"
                  >
                    <Download className="size-2.5" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </Card>
      )}

      {!file && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <FileInput className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Upload a PDF to extract pages as images.
          </p>
        </Card>
      )}

      {error && (
        <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}
    </div>
  );
}
