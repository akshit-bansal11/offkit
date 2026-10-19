"use client";

import {
  Download,
  GripVertical,
  ImageIcon,
  Info,
  Loader2,
  X,
} from "lucide-react";
import { PDFDocument } from "pdf-lib";
import React, { useCallback, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Progress } from "@/components/ui/feedback/Progress";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("convert-to-pdf");

export default function ConvertToPdfPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ConvertToPdfTool />
    </ToolPageShell>
  );
}

interface ImageFile {
  id: string;
  file: File;
  preview: string;
  name: string;
}

type PageSize = "fit" | "a4" | "letter";
type OutputMode = "combined" | "individual";

function uid() {
  return Math.random().toString(36).slice(2, 9);
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function bytesToPdfBlob(bytes: Uint8Array): Blob {
  const arrayBuffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(arrayBuffer).set(bytes);
  return new Blob([arrayBuffer], { type: "application/pdf" });
}

async function imageFileToEmbeddable(
  pdfDoc: PDFDocument,
  file: File,
): Promise<ReturnType<typeof pdfDoc.embedPng>> {
  const buf = await file.arrayBuffer();

  if (file.type === "image/png") {
    return pdfDoc.embedPng(buf);
  }
  if (file.type === "image/jpeg" || file.type === "image/jpg") {
    return pdfDoc.embedJpg(buf);
  }

  // For webp / other formats: decode via canvas → jpeg
  return new Promise((resolve, reject) => {
    const img = document.createElement("img");
    const url = URL.createObjectURL(file);
    img.onload = async () => {
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext("2d")!;
      ctx.drawImage(img, 0, 0);
      URL.revokeObjectURL(url);
      canvas.toBlob(
        async (blob) => {
          if (!blob) {
            reject(new Error("Canvas conversion failed"));
            return;
          }
          const jpgBuf = await blob.arrayBuffer();
          resolve(pdfDoc.embedJpg(jpgBuf));
        },
        "image/jpeg",
        0.92,
      );
    };
    img.onerror = () => reject(new Error("Image load failed"));
    img.src = url;
  });
}

function ConvertToPdfTool() {
  const [images, setImages] = useState<ImageFile[]>([]);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [pageSize, setPageSize] = useState<PageSize>("fit");
  const [outputMode, setOutputMode] = useState<OutputMode>("combined");
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const addImages = useCallback((incoming: File[]) => {
    const valid = incoming.filter((f) => f.type.startsWith("image/"));
    const items: ImageFile[] = valid.map((f) => ({
      id: uid(),
      file: f,
      name: f.name,
      preview: URL.createObjectURL(f),
    }));
    setImages((prev) => [...prev, ...items]);
  }, []);

  const removeImage = (id: string) => {
    setImages((prev) => {
      const item = prev.find((i) => i.id === id);
      if (item) URL.revokeObjectURL(item.preview);
      return prev.filter((i) => i.id !== id);
    });
  };

  const handleDragOver = (e: React.DragEvent, id: string) => {
    e.preventDefault();
    if (!draggingId || draggingId === id) return;
    setImages((prev) => {
      const arr = [...prev];
      const fi = arr.findIndex((i) => i.id === draggingId);
      const ti = arr.findIndex((i) => i.id === id);
      if (fi < 0 || ti < 0) return prev;
      const [moved] = arr.splice(fi, 1);
      arr.splice(ti, 0, moved);
      return arr;
    });
  };

  const buildPage = (
    pdfDoc: PDFDocument,
    img: Awaited<ReturnType<typeof pdfDoc.embedPng>>,
  ) => {
    if (pageSize === "fit") {
      const page = pdfDoc.addPage([img.width, img.height]);
      page.drawImage(img, { x: 0, y: 0, width: img.width, height: img.height });
    } else {
      const [pw, ph] = pageSize === "a4" ? [595.28, 841.89] : [612, 792];
      const page = pdfDoc.addPage([pw, ph]);
      const scale = Math.min(pw / img.width, ph / img.height);
      const w = img.width * scale;
      const h = img.height * scale;
      page.drawImage(img, {
        x: (pw - w) / 2,
        y: (ph - h) / 2,
        width: w,
        height: h,
      });
    }
  };

  const convert = async () => {
    if (images.length === 0) return;
    setIsProcessing(true);
    setProgress(0);
    setError(null);

    try {
      if (outputMode === "combined") {
        const pdfDoc = await PDFDocument.create();
        for (let i = 0; i < images.length; i++) {
          const img = await imageFileToEmbeddable(pdfDoc, images[i].file);
          buildPage(pdfDoc, img);
          setProgress(Math.round(((i + 1) / images.length) * 90));
        }
        const bytes = await pdfDoc.save();
        const blob = bytesToPdfBlob(bytes);
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "converted.pdf";
        a.click();
        URL.revokeObjectURL(url);
        setProgress(100);
      } else {
        // Individual
        for (let i = 0; i < images.length; i++) {
          const pdfDoc = await PDFDocument.create();
          const img = await imageFileToEmbeddable(pdfDoc, images[i].file);
          buildPage(pdfDoc, img);
          const bytes = await pdfDoc.save();
          const blob = bytesToPdfBlob(bytes);
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          const baseName = images[i].name.replace(/\.[^/.]+$/, "");
          a.download = `${baseName}.pdf`;
          a.click();
          URL.revokeObjectURL(url);
          setProgress(Math.round(((i + 1) / images.length) * 100));
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Conversion failed.");
    }
    setIsProcessing(false);
  };

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="images"
        supportedFormats="JPG, PNG, WEBP, GIF"
        accept="image/*"
        multiple
        onFilesSelected={addImages}
      />

      {/* Info banner */}
      <div className="flex items-start gap-3 rounded-2xl border border-info/25 bg-info/10 p-3 text-info">
        <Info className="size-4 mt-0.5 shrink-0" />
        <p className="text-xs leading-relaxed">
          Currently supports JPG, PNG, and WEBP images. DOCX/HTML conversion
          requires a server-side component.
        </p>
      </div>

      {images.length > 0 && (
        <div className="grid gap-3 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
          {images.map((item) => (
            <div
              key={item.id}
              draggable
              onDragStart={() => setDraggingId(item.id)}
              onDragOver={(e) => handleDragOver(e, item.id)}
              onDrop={(e) => {
                e.preventDefault();
                setDraggingId(null);
              }}
              className={`group relative overflow-hidden rounded-2xl border border-white/10 bg-background/45 aspect-square cursor-grab active:cursor-grabbing transition-all ${draggingId === item.id ? "opacity-40" : "hover:border-white/20"}`}
            >
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src={item.preview}
                alt={item.name}
                className="h-full w-full object-cover"
              />
              <div className="absolute inset-x-0 bottom-0 bg-black/70 px-2 py-1.5">
                <p className="truncate text-[9px] text-white/80">{item.name}</p>
              </div>
              <button
                onClick={() => removeImage(item.id)}
                className="absolute right-1.5 top-1.5 rounded-full bg-black/60 p-0.5 text-white/60 opacity-0 transition-opacity group-hover:opacity-100 hover:text-white"
              >
                <X className="size-3" />
              </button>
              <div className="absolute left-1.5 top-1.5 rounded-md bg-black/50 p-0.5 text-white/60 opacity-0 transition-opacity group-hover:opacity-100">
                <GripVertical className="size-3" />
              </div>
            </div>
          ))}
        </div>
      )}

      {images.length === 0 && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <ImageIcon className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Add images to convert to PDF.
          </p>
        </Card>
      )}

      {images.length >= 1 && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="grid gap-5 p-5 sm:grid-cols-2">
            <div className="space-y-2">
              <p className="text-sm font-medium">Output mode</p>
              <SegmentedControl
                variant="dark"
                value={outputMode}
                onValueChange={(v) => setOutputMode(v as OutputMode)}
                className="rounded-xl border bg-card/60 p-1"
                optionClassName="rounded-lg px-4 py-1.5 text-xs"
                options={[
                  { label: "Combined PDF", value: "combined" },
                  { label: "Individual PDFs", value: "individual" },
                ]}
              />
            </div>

            <div className="space-y-2">
              <p className="text-sm font-medium">Page size</p>
              <Select
                options={[
                  { label: "Fit to image", value: "fit" },
                  { label: "A4 (595 × 842 pt)", value: "a4" },
                  { label: "Letter (612 × 792 pt)", value: "letter" },
                ]}
                value={pageSize}
                onChange={(e) => setPageSize(e.target.value as PageSize)}
                className="h-9 border-white/10 bg-white/[0.04] text-sm"
              />
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-3 px-5 py-3">
            <p className="text-xs text-muted-foreground">
              <span className="font-mono">{images.length}</span> image
              {images.length !== 1 ? "s" : ""} ·{" "}
              <span className="font-mono">
                {images
                  .reduce((s, i) => s + i.file.size, 0)
                  .toLocaleString()}{" "}
              </span>
              <span className="text-muted-foreground/60">
                ({formatFileSize(images.reduce((s, i) => s + i.file.size, 0))})
              </span>
            </p>

            {isProcessing && (
              <div className="flex-1">
                <Progress value={progress} className="h-1.5 bg-white/10" />
              </div>
            )}

            <div className="ml-auto flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                className="h-8 border border-white/10 text-xs text-muted-foreground"
                onClick={() => {
                  images.forEach((i) => URL.revokeObjectURL(i.preview));
                  setImages([]);
                }}
                disabled={isProcessing}
              >
                Clear
              </Button>
              <Button
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={isProcessing || images.length === 0}
                onClick={() => void convert()}
              >
                {isProcessing ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Download className="size-3.5" />
                )}
                {isProcessing ? "Converting…" : "Convert to PDF"}
              </Button>
            </div>
          </div>
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
