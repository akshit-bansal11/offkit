"use client";

import {
  Download,
  File,
  GripVertical,
  Loader2,
  Merge,
  Trash2,
  X,
} from "lucide-react";
import { PDFDocument } from "pdf-lib";
import * as pdfjsLib from "pdfjs-dist/legacy/build/pdf.mjs";
import React, { useCallback, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";

if (typeof window !== "undefined") {
  pdfjsLib.GlobalWorkerOptions.workerSrc = `//cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjsLib.version}/pdf.worker.min.mjs`;
}

const tool = getToolBySlug("pdf-merger");

export default function PdfMergerPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <PdfMergerTool />
    </ToolPageShell>
  );
}

interface MergeFile {
  id: string;
  file: File;
  name: string;
  size: number;
  pageCount: number | null;
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

function uid() {
  return Math.random().toString(36).slice(2, 9);
}

function PdfMergerTool() {
  const [files, setFiles] = useState<MergeFile[]>([]);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [outputUrl, setOutputUrl] = useState<string | null>(null);
  const [outputSize, setOutputSize] = useState(0);

  const addFiles = useCallback(async (incoming: File[]) => {
    const valid = incoming.filter((f) => f.type === "application/pdf");
    if (valid.length === 0) return;

    const newItems: MergeFile[] = valid.map((f) => ({
      id: uid(),
      file: f,
      name: f.name,
      size: f.size,
      pageCount: null,
    }));

    setFiles((prev) => [...prev, ...newItems]);

    // Load page counts asynchronously
    for (const item of newItems) {
      try {
        const buf = await item.file.arrayBuffer();
        const pdf = await pdfjsLib.getDocument({ data: buf }).promise;
        setFiles((prev) =>
          prev.map((f) =>
            f.id === item.id ? { ...f, pageCount: pdf.numPages } : f,
          ),
        );
      } catch {
        // page count unavailable
      }
    }
  }, []);

  const removeFile = (id: string) => {
    setFiles((prev) => prev.filter((f) => f.id !== id));
  };

  const handleDragStart = (id: string) => setDraggingId(id);
  const handleDragOver = (e: React.DragEvent, id: string) => {
    e.preventDefault();
    if (!draggingId || draggingId === id) return;
    setFiles((prev) => {
      const arr = [...prev];
      const fi = arr.findIndex((f) => f.id === draggingId);
      const ti = arr.findIndex((f) => f.id === id);
      if (fi < 0 || ti < 0) return prev;
      const [moved] = arr.splice(fi, 1);
      arr.splice(ti, 0, moved);
      return arr;
    });
  };
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDraggingId(null);
  };

  const merge = async () => {
    if (files.length < 2) return;
    setIsProcessing(true);
    setProgress(0);
    setError(null);
    if (outputUrl) URL.revokeObjectURL(outputUrl);
    setOutputUrl(null);

    try {
      const merged = await PDFDocument.create();
      for (let i = 0; i < files.length; i++) {
        const buf = await files[i].file.arrayBuffer();
        const pdf = await PDFDocument.load(buf);
        const copied = await merged.copyPages(pdf, pdf.getPageIndices());
        copied.forEach((p) => merged.addPage(p));
        setProgress(Math.round(((i + 1) / files.length) * 90));
      }
      const bytes = await merged.save();
      const blob = bytesToPdfBlob(bytes);
      setOutputUrl(URL.createObjectURL(blob));
      setOutputSize(blob.size);
      setProgress(100);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to merge PDFs.");
    }
    setIsProcessing(false);
  };

  const download = () => {
    if (!outputUrl) return;
    const a = document.createElement("a");
    a.href = outputUrl;
    a.download = "merged.pdf";
    a.click();
  };

  const totalPages = files.reduce((s, f) => s + (f.pageCount ?? 0), 0);

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="PDF files"
        supportedFormats="PDF"
        accept="application/pdf"
        multiple
        onFilesSelected={(f) => void addFiles(f)}
      />

      {files.length > 0 && (
        <div className="space-y-2">
          {files.map((item) => (
            <div
              key={item.id}
              draggable
              onDragStart={() => handleDragStart(item.id)}
              onDragOver={(e) => handleDragOver(e, item.id)}
              onDrop={handleDrop}
              className={`group flex items-center gap-3 rounded-2xl border border-white/10 bg-background/45 p-3 transition-colors hover:border-white/15 cursor-grab active:cursor-grabbing ${draggingId === item.id ? "opacity-40" : ""}`}
            >
              <GripVertical className="size-4 shrink-0 text-white/25 group-hover:text-white/50" />
              <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-400">
                <File className="size-5" />
              </div>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium" title={item.name}>
                  {item.name}
                </p>
                <div className="mt-0.5 flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">
                    {formatFileSize(item.size)}
                  </span>
                  {item.pageCount !== null && (
                    <Badge
                      variant="outline"
                      className="border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px]"
                    >
                      {item.pageCount}p
                    </Badge>
                  )}
                </div>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 shrink-0 text-muted-foreground"
                onClick={() => removeFile(item.id)}
                disabled={isProcessing}
              >
                <X className="size-4" />
              </Button>
            </div>
          ))}
        </div>
      )}

      {files.length === 0 && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <Merge className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Add two or more PDF files to get started.
          </p>
        </Card>
      )}

      {files.length >= 1 && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="flex flex-wrap items-center gap-4 px-5 py-3">
            <div className="text-xs text-muted-foreground">
              <span className="font-mono">{files.length}</span> files
              {totalPages > 0 && (
                <>
                  {" · "}
                  <span className="font-mono">{totalPages}</span> pages total
                </>
              )}
            </div>

            <div className="ml-auto flex items-center gap-2">
              {files.length > 1 && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 gap-1.5 border border-white/10 text-xs text-muted-foreground"
                  onClick={() => setFiles([])}
                  disabled={isProcessing}
                >
                  <Trash2 className="size-3.5" />
                  Clear
                </Button>
              )}
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
                disabled={isProcessing || files.length < 2}
                onClick={() => void merge()}
              >
                {isProcessing ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Merge className="size-3.5" />
                )}
                {isProcessing ? "Merging…" : "Merge PDFs"}
              </Button>
            </div>
          </div>

          {(isProcessing || progress === 100) && (
            <div className="space-y-2 px-5 pb-4 pt-3">
              <Progress value={progress} className="h-1.5 bg-white/10" />
              {progress === 100 && outputSize > 0 && (
                <p className="text-xs text-muted-foreground">
                  Output: {formatFileSize(outputSize)} · PDF
                </p>
              )}
            </div>
          )}
        </Card>
      )}

      {error && (
        <div className="rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-sm text-red-200">
          {error}
        </div>
      )}
    </div>
  );
}
