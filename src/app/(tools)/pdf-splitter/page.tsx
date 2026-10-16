"use client";

import JSZip from "jszip";
import {
  Download,
  File,
  Loader2,
  SplitSquareHorizontal,
  X,
} from "lucide-react";
import { PDFDocument } from "pdf-lib";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Progress } from "@/components/ui/feedback/Progress";
import { Input } from "@/components/ui/form/Input";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { getToolBySlug } from "@/config/tools";
import { pdfjsLib } from "@/lib/pdf/pdfjs";
import {
  describePageRangeProblem,
  formatFileSize,
  parsePageRanges,
} from "@/lib/tools/pdf-toolkit/utils";

const tool = getToolBySlug("pdf-splitter");

export default function PdfSplitterPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <PdfSplitterTool />
    </ToolPageShell>
  );
}

type SplitMode = "every" | "range" | "fixed";

interface PageThumb {
  index: number;
  url: string;
}

function bytesToPdfBlob(bytes: Uint8Array): Blob {
  const arrayBuffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(arrayBuffer).set(bytes);
  return new Blob([arrayBuffer], { type: "application/pdf" });
}

function PdfSplitterTool() {
  const [file, setFile] = useState<File | null>(null);
  const [pageCount, setPageCount] = useState(0);
  const [thumbnails, setThumbnails] = useState<PageThumb[]>([]);
  const [selectedPages, setSelectedPages] = useState<Set<number>>(new Set());
  const [splitMode, setSplitMode] = useState<SplitMode>("every");
  const [splitRange, setSplitRange] = useState("1-3");
  const [chunkSize, setChunkSize] = useState("5");
  const [isProcessing, setIsProcessing] = useState(false);
  const [isLoadingThumbs, setIsLoadingThumbs] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const thumbUrlsRef = useRef<string[]>([]);

  useEffect(() => {
    return () => {
      thumbUrlsRef.current.forEach((u) => URL.revokeObjectURL(u));
    };
  }, []);

  const loadFile = useCallback(async (f: File) => {
    setFile(f);
    setError(null);
    setSelectedPages(new Set());
    setIsLoadingThumbs(true);

    thumbUrlsRef.current.forEach((u) => URL.revokeObjectURL(u));
    thumbUrlsRef.current = [];
    setThumbnails([]);

    try {
      const buf = await f.arrayBuffer();
      const pdf = await pdfjsLib.getDocument({ data: buf }).promise;
      setPageCount(pdf.numPages);

      const thumbs: PageThumb[] = [];
      for (let i = 1; i <= pdf.numPages; i++) {
        const page = await pdf.getPage(i);
        const vp = page.getViewport({ scale: 0.4 });
        const canvas = document.createElement("canvas");
        const ctx = canvas.getContext("2d")!;
        canvas.width = vp.width;
        canvas.height = vp.height;
        // @ts-expect-error — pdfjs typings
        await page.render({ canvasContext: ctx, viewport: vp }).promise;
        const blob = await new Promise<Blob | null>((res) =>
          canvas.toBlob(res, "image/jpeg", 0.7),
        );
        if (blob) {
          const url = URL.createObjectURL(blob);
          thumbUrlsRef.current.push(url);
          thumbs.push({ index: i - 1, url });
          setThumbnails([...thumbs]);
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to read PDF.");
    }
    setIsLoadingThumbs(false);
  }, []);

  const downloadBlob = (blob: Blob, name: string) => {
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
  };

  const split = async () => {
    if (!file) return;
    setIsProcessing(true);
    setProgress(0);
    setError(null);
    try {
      const buf = await file.arrayBuffer();
      const pdf = await PDFDocument.load(buf);
      const total = pdf.getPageCount();

      if (splitMode === "every") {
        const zip = new JSZip();
        for (let i = 0; i < total; i++) {
          const newPdf = await PDFDocument.create();
          const [pg] = await newPdf.copyPages(pdf, [i]);
          newPdf.addPage(pg);
          zip.file(`Page_${i + 1}.pdf`, await newPdf.save());
          setProgress(Math.round(((i + 1) / total) * 90));
        }
        const blob = await zip.generateAsync({ type: "blob" });
        downloadBlob(blob, `${file.name.replace(".pdf", "")}_split.zip`);
        setProgress(100);
      } else if (splitMode === "range") {
        // Picked thumbnails override the typed range.
        const selection =
          selectedPages.size > 0
            ? {
                pages: Array.from(selectedPages).sort((a, b) => a - b),
                invalid: [],
              }
            : parsePageRanges(splitRange, total);
        const problem = describePageRangeProblem(selection, total);
        if (problem) {
          setError(problem);
          setIsProcessing(false);
          return;
        }
        const newPdf = await PDFDocument.create();
        const pages = await newPdf.copyPages(pdf, selection.pages);
        pages.forEach((p) => newPdf.addPage(p));
        const bytes = await newPdf.save();
        downloadBlob(bytesToPdfBlob(bytes), "Extracted_Pages.pdf");
        setProgress(100);
      } else {
        // fixed chunks
        const cs = Math.max(1, parseInt(chunkSize, 10) || 5);
        const zip = new JSZip();
        for (let start = 0; start < total; start += cs) {
          const end = Math.min(start + cs, total);
          const newPdf = await PDFDocument.create();
          const indices = Array.from(
            { length: end - start },
            (_, k) => start + k,
          );
          const pages = await newPdf.copyPages(pdf, indices);
          pages.forEach((p) => newPdf.addPage(p));
          zip.file(`Pages_${start + 1}-${end}.pdf`, await newPdf.save());
          setProgress(Math.round((end / total) * 90));
        }
        const blob = await zip.generateAsync({ type: "blob" });
        downloadBlob(blob, `${file.name.replace(".pdf", "")}_chunks.zip`);
        setProgress(100);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to split PDF.");
    }
    setIsProcessing(false);
  };

  const togglePage = (idx: number) => {
    setSelectedPages((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
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
          if (f) void loadFile(f);
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
              {formatFileSize(file.size)} · {pageCount} pages
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="size-8 text-muted-foreground"
            onClick={() => {
              setFile(null);
              setThumbnails([]);
              setPageCount(0);
            }}
          >
            <X className="size-4" />
          </Button>
        </div>
      )}

      {file && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="space-y-4 p-5">
            <div className="flex items-center gap-3">
              <p className="text-sm font-medium text-foreground">Split mode</p>
            </div>
            <SegmentedControl
              variant="dark"
              value={splitMode}
              onValueChange={(v) => {
                setSplitMode(v as SplitMode);
                setSelectedPages(new Set());
              }}
              className="max-w-sm rounded-xl border bg-card/60 p-1"
              optionClassName="rounded-lg px-4 py-1.5 text-xs"
              options={[
                { label: "Every page (ZIP)", value: "every" },
                { label: "Custom range", value: "range" },
                { label: "Fixed chunks", value: "fixed" },
              ]}
            />

            {splitMode === "range" && (
              <div className="space-y-2">
                <p className="text-xs text-muted-foreground">
                  Enter a range (e.g. 1-3, 5) or click pages below to select
                </p>
                <Input
                  value={splitRange}
                  onChange={(e) => {
                    setSplitRange(e.target.value);
                    setSelectedPages(new Set());
                  }}
                  placeholder="e.g. 1-3, 5, 8-10"
                  aria-label="Page range"
                  className="h-9 max-w-xs border-white/10 bg-white/[0.04] font-mono text-sm"
                />
              </div>
            )}

            {splitMode === "fixed" && (
              <div className="flex items-center gap-3">
                <p className="text-xs text-muted-foreground">
                  Pages per chunk:
                </p>
                <Input
                  type="number"
                  min={1}
                  value={chunkSize}
                  onChange={(e) => setChunkSize(e.target.value)}
                  className="h-9 w-20 border-white/10 bg-white/[0.04] text-sm"
                />
              </div>
            )}
          </div>

          {/* Page thumbnail picker */}
          {(splitMode === "range" || splitMode === "fixed") && (
            <div className="p-5">
              {isLoadingThumbs ? (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="size-4 animate-spin" />
                  Generating previews…
                </div>
              ) : (
                <div className="grid grid-cols-4 gap-3 sm:grid-cols-6 md:grid-cols-8 lg:grid-cols-10">
                  {thumbnails.map((t) => (
                    <button
                      key={t.index}
                      onClick={() =>
                        splitMode === "range" && togglePage(t.index)
                      }
                      className={`relative overflow-hidden rounded-xl border aspect-[1/1.41] transition-all ${
                        selectedPages.has(t.index)
                          ? "border-emerald-400 ring-1 ring-emerald-400"
                          : "border-white/10 hover:border-white/30"
                      } ${splitMode !== "range" ? "cursor-default" : "cursor-pointer"}`}
                    >
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img
                        src={t.url}
                        alt={`Page ${t.index + 1}`}
                        className="h-full w-full object-contain bg-white"
                      />
                      <div className="absolute bottom-0 inset-x-0 bg-black/60 py-0.5 text-center text-[9px] text-white">
                        {t.index + 1}
                      </div>
                      {selectedPages.has(t.index) && (
                        <div className="absolute top-1 right-1 size-3 rounded-full bg-emerald-400" />
                      )}
                    </button>
                  ))}
                </div>
              )}
              {splitMode === "range" && selectedPages.size > 0 && (
                <p className="mt-2 text-xs text-muted-foreground">
                  {selectedPages.size} page
                  {selectedPages.size !== 1 ? "s" : ""} selected
                </p>
              )}
            </div>
          )}

          <div className="flex items-center gap-3 px-5 py-3">
            {isProcessing && (
              <div className="flex-1 space-y-1">
                <Progress value={progress} className="h-1.5 bg-white/10" />
              </div>
            )}
            <div className="ml-auto">
              <Button
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={isProcessing || !file}
                onClick={() => void split()}
              >
                {isProcessing ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Download className="size-3.5" />
                )}
                {isProcessing ? "Processing…" : "Split & Download"}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {!file && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <SplitSquareHorizontal className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Upload a PDF to get started.
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
