"use client";

import { ToolPageShell } from "@/components/common/ToolPageShell";
import { getToolBySlug } from "@/config/tools";
import React, { useState, useCallback, useEffect } from "react";
import {
  Merge,
  SplitSquareHorizontal,
  Minimize2,
  ArrowUpDown,
  File,
  X,
  GripVertical,
  Download,
  Trash2,
  RefreshCw,
  Settings,
  Image as ImageIcon,
} from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/layout/Card";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/form/Input";
import { Badge } from "@/components/ui/feedback/Badge";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Slider } from "@/components/ui/Slider";
import { Progress } from "@/components/ui/feedback/Progress";
import { Label } from "@/components/ui/form/Label";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { PDFDocument } from "pdf-lib";
import * as pdfjsLib from "pdfjs-dist/legacy/build/pdf.mjs";
import JSZip from "jszip";
import type { PdfFileItem, PdfPageThumbnail, TabType } from "@/lib/tools/pdf-toolkit/types";
import { formatFileSize, generateId, parseRanges } from "@/lib/tools/pdf-toolkit/utils";

const tool = getToolBySlug("pdf-toolkit");

export default function PdfToolkitPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell
      title={tool.name}
      description={tool.description}
     
    >
      <PdfToolkitTool />
    </ToolPageShell>
  );
}


if (typeof window !== "undefined") {
  pdfjsLib.GlobalWorkerOptions.workerSrc = `//cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjsLib.version}/pdf.worker.min.mjs`;
}


function PdfToolkitTool() {
  const [activeTab, setActiveTab] = useState<TabType>("merge");

  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // --- MERGE STATE ---
  const [mergeFiles, setMergeFiles] = useState<PdfFileItem[]>([]);
  const [draggedMergeId, setDraggedMergeId] = useState<string | null>(null);

  // --- SPLIT STATE ---
  const [splitFile, setSplitFile] = useState<File | null>(null);
  const [splitMode, setSplitMode] = useState<"every" | "range">("every");
  const [splitRange, setSplitRange] = useState("1-3, 5");
  const [splitTotalPages, setSplitTotalPages] = useState(0);

  // --- COMPRESS STATE ---
  const [compressFile, setCompressFile] = useState<File | null>(null);
  const [compressQuality, setCompressQuality] = useState<number>(60);
  const [compressResolution, setCompressResolution] = useState<number>(1.5); // 1.0 to 2.0 scaled

  // --- REORDER STATE ---
  const [reorderFile, setReorderFile] = useState<File | null>(null);
  const [reorderPages, setReorderPages] = useState<PdfPageThumbnail[]>([]);
  const [draggedReorderId, setDraggedReorderId] = useState<string | null>(null);

  // Cleanups
  useEffect(() => {
    return () => {
      reorderPages.forEach((p) => URL.revokeObjectURL(p.previewUrl));
    };
  }, [reorderPages]);

  const loadPdfMetadata = async (file: File) => {
    try {
      const buffer = await file.arrayBuffer();
      const pdf = await pdfjsLib.getDocument({ data: buffer }).promise;
      return pdf;
    } catch (e) {
      console.error(e);
      return null;
    }
  };

  const processUpload = useCallback(
    async (files: File[]) => {
      const validPdfs = files.filter((f) => f.type === "application/pdf");
      if (validPdfs.length === 0) return;

      if (activeTab === "merge") {
        setMergeFiles((prev) => [
          ...prev,
          ...validPdfs.map((f) => ({
            id: generateId(),
            file: f,
            name: f.name,
            size: f.size,
          })),
        ]);
      } else if (activeTab === "split") {
        const file = validPdfs[0];
        setSplitFile(file);
        const pdf = await loadPdfMetadata(file);
        if (pdf) setSplitTotalPages(pdf.numPages);
      } else if (activeTab === "compress") {
        setCompressFile(validPdfs[0]);
      } else if (activeTab === "reorder") {
        const file = validPdfs[0];
        setReorderFile(file);
        setReorderPages([]); // clear old
        setIsProcessing(true);
        setProgress(0);

        const pdf = await loadPdfMetadata(file);
        if (pdf) {
          const total = pdf.numPages;
          const thumbnails: PdfPageThumbnail[] = [];
          for (let i = 1; i <= total; i++) {
            const page = await pdf.getPage(i);
            const viewport = page.getViewport({ scale: 0.5 }); // Low res for thumbnail
            const canvas = document.createElement("canvas");
            const ctx = canvas.getContext("2d");
            canvas.width = viewport.width;
            canvas.height = viewport.height;
            if (ctx) {
              // @ts-expect-error - The pdfjs-dist TS typings strictly require canvas sometimes improperly despite canvasContext being the actual parameter
              await page.render({ canvasContext: ctx, viewport }).promise;
              const blob = await new Promise<Blob | null>((res) =>
                canvas.toBlob(res, "image/jpeg", 0.7),
              );
              if (blob)
                thumbnails.push({
                  id: generateId(),
                  originalIndex: i - 1,
                  previewUrl: URL.createObjectURL(blob),
                });
            }
            setProgress(Math.round((i / total) * 100));
          }
          setReorderPages(thumbnails);
        }
        setIsProcessing(false);
        setProgress(0);
      }
    },
    [activeTab],
  );

  const downloadBlob = (blob: Blob, name: string) => {
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
  };

  // --- TAB PROCESSORS ---
  const executeMerge = async () => {
    if (mergeFiles.length === 0) return;
    setIsProcessing(true);
    setProgress(0);
    setErrorMessage(null);
    try {
      const mergedPdf = await PDFDocument.create();
      for (let i = 0; i < mergeFiles.length; i++) {
        const fileBuffer = await mergeFiles[i].file.arrayBuffer();
        const pdf = await PDFDocument.load(fileBuffer);
        const copiedPages = await mergedPdf.copyPages(
          pdf,
          pdf.getPageIndices(),
        );
        copiedPages.forEach((p) => mergedPdf.addPage(p));
        setProgress(Math.round(((i + 1) / mergeFiles.length) * 100));
      }
      const bytes = await mergedPdf.save();
      const buffer = Uint8Array.from(bytes).buffer;
      downloadBlob(
        new Blob([buffer], { type: "application/pdf" }),
        "Merged_Document.pdf",
      );
    } catch (e) {
      console.error(e);
      setErrorMessage("Error merging documents.");
    }
    setIsProcessing(false);
    setProgress(0);
  };

  const executeSplit = async () => {
    if (!splitFile) return;
    setIsProcessing(true);
    setProgress(0);
    setErrorMessage(null);
    try {
      const fileBuffer = await splitFile.arrayBuffer();
      const pdf = await PDFDocument.load(fileBuffer);
      const total = pdf.getPageCount();

      if (splitMode === "every") {
        const zip = new JSZip();
        for (let i = 0; i < total; i++) {
          const newPdf = await PDFDocument.create();
          const [page] = await newPdf.copyPages(pdf, [i]);
          newPdf.addPage(page);
          zip.file(`Page_${i + 1}.pdf`, await newPdf.save());
          setProgress(Math.round(((i + 1) / total) * 100));
        }
        const blob = await zip.generateAsync({ type: "blob" });
        downloadBlob(blob, `${splitFile.name.replace(".pdf", "")}_split.zip`);
      } else {
        const indices = parseRanges(splitRange, total);
        if (indices.length === 0) {
          setErrorMessage("Invalid or empty range.");
          setIsProcessing(false);
          return;
        }
        const newPdf = await PDFDocument.create();
        const pages = await newPdf.copyPages(pdf, indices);
        pages.forEach((p) => newPdf.addPage(p));
        const bytes = await newPdf.save();
        const buffer = Uint8Array.from(bytes).buffer;
        downloadBlob(
          new Blob([buffer], { type: "application/pdf" }),
          "Extracted_Pages.pdf",
        );
      }
    } catch (e) {
      console.error(e);
      setErrorMessage("Failed to split document.");
    }
    setIsProcessing(false);
    setProgress(0);
  };

  const executeCompress = async () => {
    if (!compressFile) return;
    setIsProcessing(true);
    setProgress(0);
    setErrorMessage(null);
    try {
      // Rasterization approach (Flattens PDF but significantly drops sizes of image-heavy docs safely)
      const fileBuffer = await compressFile.arrayBuffer();
      const pdf = await pdfjsLib.getDocument({ data: fileBuffer }).promise;
      const total = pdf.numPages;

      const newPdf = await PDFDocument.create();
      for (let i = 1; i <= total; i++) {
        const page = await pdf.getPage(i);
        const viewport = page.getViewport({ scale: compressResolution });
        const canvas = document.createElement("canvas");
        const ctx = canvas.getContext("2d");
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        if (ctx) {
          await page.render({ canvas, canvasContext: ctx, viewport }).promise;
          const blob = await new Promise<Blob | null>((res) =>
            canvas.toBlob(res, "image/jpeg", compressQuality / 100),
          );
          if (blob) {
            const imgBytes = await blob.arrayBuffer();
            const jpegImg = await newPdf.embedJpg(imgBytes);
            const newPage = newPdf.addPage([jpegImg.width, jpegImg.height]);
            newPage.drawImage(jpegImg, {
              x: 0,
              y: 0,
              width: jpegImg.width,
              height: jpegImg.height,
            });
          }
        }
        setProgress(Math.round((i / total) * 100));
      }
      const bytes = await newPdf.save();
      const buffer = Uint8Array.from(bytes).buffer;
      downloadBlob(
        new Blob([buffer], { type: "application/pdf" }),
        "Compressed_Document.pdf",
      );
    } catch (e) {
      console.error(e);
      setErrorMessage("Failed to compress document.");
    }
    setIsProcessing(false);
    setProgress(0);
  };

  const executeReorder = async () => {
    if (!reorderFile || reorderPages.length === 0) return;
    setIsProcessing(true);
    setProgress(0);
    setErrorMessage(null);
    try {
      const fileBuffer = await reorderFile.arrayBuffer();
      const pdf = await PDFDocument.load(fileBuffer);
      const newPdf = await PDFDocument.create();

      const orderedIndices = reorderPages.map((p) => p.originalIndex);
      const copied = await newPdf.copyPages(pdf, orderedIndices);
      copied.forEach((p) => newPdf.addPage(p));

      const bytes = await newPdf.save();
      const buffer = Uint8Array.from(bytes).buffer;
      downloadBlob(
        new Blob([buffer], { type: "application/pdf" }),
        "Reordered_Document.pdf",
      );
    } catch (e) {
      console.error(e);
      setErrorMessage("Failed to reorder document.");
    }
    setIsProcessing(false);
    setProgress(0);
  };

  return (
    <div className="space-y-6 max-w-5xl mx-auto xl:max-w-7xl">
      {/* Tabs / Mode Selector */}
      <div className="flex w-full items-center justify-center">
        <SegmentedControl
          variant="dark"
          value={activeTab}
          onValueChange={(value) => {
            setActiveTab(value as TabType);
            setErrorMessage(null);
          }}
          className="max-w-full rounded-full border bg-card/60 p-1.5 shadow-sm backdrop-blur-md"
          optionClassName="rounded-full whitespace-nowrap px-5 py-2.5 text-sm"
          options={[
            {
              label: (
                <span className="flex items-center gap-2">
                  <Merge className="size-4" /> Merge
                </span>
              ),
              value: "merge",
            },
            {
              label: (
                <span className="flex items-center gap-2">
                  <SplitSquareHorizontal className="size-4" /> Split
                </span>
              ),
              value: "split",
            },
            {
              label: (
                <span className="flex items-center gap-2">
                  <Minimize2 className="size-4" /> Compress
                </span>
              ),
              value: "compress",
            },
            {
              label: (
                <span className="flex items-center gap-2">
                  <ArrowUpDown className="size-4" /> Reorder
                </span>
              ),
              value: "reorder",
            },
          ]}
        />
      </div>

      {errorMessage ? (
        <div
          role="alert"
          className="rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-sm text-red-200"
        >
          {errorMessage}
        </div>
      ) : null}

      <div className="grid gap-6 lg:grid-cols-[1fr_300px] xl:grid-cols-[1fr_360px]">
        {/* Main Interface Area */}
        <div className="space-y-6">
          <FileDropZoneCard
            fileTypeLabel={activeTab === "merge" ? "PDF files" : "a PDF file"}
            supportedFormats="PDF"
            accept="application/pdf"
            multiple={activeTab === "merge"}
            onFilesSelected={(files) => {
              processUpload(files);
            }}
          />

          {/* MERGE VIEW: List of PDFs */}
          {activeTab === "merge" && mergeFiles.length > 0 && (
            <div className="space-y-4 rounded-3xl border bg-card/40 p-6">
              <div className="flex items-center justify-between border-b pb-4 border-white/5">
                <h3 className="text-lg font-semibold tracking-tight text-foreground">
                  Files to Merge{" "}
                  <span className="text-muted-foreground font-normal ml-2">
                    ({mergeFiles.length})
                  </span>
                </h3>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setMergeFiles([])}
                  className="text-muted-foreground hover:text-red-400"
                >
                  <Trash2 className="size-4 mr-2" />
                  Clear All
                </Button>
              </div>
              <div className="grid gap-3 pt-2">
                {mergeFiles.map((item) => (
                  <div
                    key={item.id}
                    draggable
                    onDragStart={(e) => {
                      setDraggedMergeId(item.id);
                      e.dataTransfer.effectAllowed = "move";
                    }}
                    onDragOver={(e) => {
                      e.preventDefault();
                      e.dataTransfer.dropEffect = "move";
                    }}
                    onDrop={(e) => {
                      e.preventDefault();
                      if (!draggedMergeId || draggedMergeId === item.id) return;
                      setMergeFiles((prev) => {
                        const arr = [...prev];
                        const oldIdx = arr.findIndex(
                          (i) => i.id === draggedMergeId,
                        );
                        const newIdx = arr.findIndex((i) => i.id === item.id);
                        const [moved] = arr.splice(oldIdx, 1);
                        arr.splice(newIdx, 0, moved);
                        return arr;
                      });
                      setDraggedMergeId(null);
                    }}
                    className={`group relative flex items-center overflow-hidden rounded-2xl border bg-background/50 pr-2 transition-colors cursor-grab active:cursor-grabbing ${
                      draggedMergeId === item.id
                        ? "opacity-30"
                        : "hover:border-primary/50"
                    }`}
                  >
                    <div className="px-3 text-muted-foreground/30 transition-colors group-hover:text-muted-foreground h-full flex items-center bg-black/10 py-4">
                      <GripVertical className="size-4" />
                    </div>
                    <div className="p-3 text-red-500/80">
                      <File className="size-6" />
                    </div>
                    <div className="flex-1 min-w-0 py-3 select-none">
                      <p className="truncate text-sm font-medium text-foreground">
                        {item.name}
                      </p>
                      <p className="truncate text-xs text-muted-foreground mt-0.5">
                        {formatFileSize(item.size)}
                      </p>
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      onClick={(e) => {
                        e.stopPropagation();
                        setMergeFiles((prev) =>
                          prev.filter((i) => i.id !== item.id),
                        );
                      }}
                      className="h-8 w-8 p-0 opacity-0 text-muted-foreground transition-opacity group-hover:opacity-100 hover:text-red-400"
                    >
                      <X className="size-4" />
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* SINGLE DOC VIEW: Split, Compress, Reorder Context */}
          {activeTab !== "merge" &&
            (splitFile || compressFile || reorderFile) && (
              <div className="flex items-center gap-4 bg-background/50 p-4 rounded-3xl border shadow-sm">
                <div className="p-3 bg-red-500/10 rounded-xl text-red-500">
                  <File className="size-6" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="font-semibold text-foreground truncate">
                    {(activeTab === "split" && splitFile?.name) ||
                      (activeTab === "compress" && compressFile?.name) ||
                      (activeTab === "reorder" && reorderFile?.name)}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    {formatFileSize(
                      (activeTab === "split" && splitFile?.size) ||
                        (activeTab === "compress" && compressFile?.size) ||
                        (activeTab === "reorder" && reorderFile?.size) ||
                        0,
                    )}
                    {activeTab === "split" && splitTotalPages > 0
                      ? ` • ${splitTotalPages} Pages`
                      : ""}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => {
                    if (activeTab === "split") setSplitFile(null);
                    if (activeTab === "compress") setCompressFile(null);
                    if (activeTab === "reorder") {
                      setReorderFile(null);
                      setReorderPages([]);
                    }
                  }}
                  className="text-muted-foreground hover:text-red-400"
                >
                  <X className="size-5" />
                </Button>
              </div>
            )}

          {/* REORDER VIEW: Thumbnail Grid */}
          {activeTab === "reorder" && reorderPages.length > 0 && (
            <div className="space-y-4 rounded-3xl border bg-card/40 p-6">
              <h3 className="text-lg font-semibold tracking-tight border-b pb-4 border-white/5">
                Drag to Reorder Pages
              </h3>
              <div className="grid gap-4 sm:grid-cols-3 xl:grid-cols-4 pt-2">
                {reorderPages.map((page) => (
                  <div
                    key={page.id}
                    draggable
                    onDragStart={(e) => {
                      setDraggedReorderId(page.id);
                      e.dataTransfer.effectAllowed = "move";
                    }}
                    onDragOver={(e) => {
                      e.preventDefault();
                      e.dataTransfer.dropEffect = "move";
                    }}
                    onDrop={(e) => {
                      e.preventDefault();
                      if (!draggedReorderId || draggedReorderId === page.id)
                        return;
                      setReorderPages((prev) => {
                        const arr = [...prev];
                        const oldIdx = arr.findIndex(
                          (i) => i.id === draggedReorderId,
                        );
                        const newIdx = arr.findIndex((i) => i.id === page.id);
                        const [moved] = arr.splice(oldIdx, 1);
                        arr.splice(newIdx, 0, moved);
                        return arr;
                      });
                      setDraggedReorderId(null);
                    }}
                    className={`group relative rounded-2xl border bg-background/50 p-2 overflow-hidden shadow-sm cursor-grab active:cursor-grabbing transition-colors ${
                      draggedReorderId === page.id
                        ? "opacity-30 border-primary"
                        : "hover:border-primary/50"
                    }`}
                  >
                    <div className="aspect-[1/1.414] w-full overflow-hidden rounded-xl bg-white border pointer-events-none">
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img
                        src={page.previewUrl}
                        alt={`Page ${page.originalIndex + 1}`}
                        className="h-full w-full object-contain"
                      />
                    </div>
                    <div className="absolute top-4 left-4 pointer-events-none">
                      <Badge
                        variant="secondary"
                        className="bg-black/80 text-white backdrop-blur shadow-md font-mono"
                      >
                        {page.originalIndex + 1}
                      </Badge>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Sidebar settings */}
        <div className="space-y-6">
          <Card className="tool-card-inline sticky top-6 lg:top-8">
            <CardHeader className="border-b border-white/5 pb-4">
              <CardTitle className="flex items-center gap-2 text-lg">
                <Settings className="size-5 text-muted-foreground" />
                Toolkit Config
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-6 space-y-6">
              {activeTab === "merge" && (
                <>
                  <p className="text-sm text-muted-foreground leading-relaxed">
                    Merge multiple PDF documents sequentially into one single
                    file. Rearrange the order in the list on the left.
                  </p>
                  <Button
                    onClick={executeMerge}
                    disabled={mergeFiles.length < 2 || isProcessing}
                    className="w-full gap-2 h-12 rounded-xl shadow-lg font-semibold btn-accent"
                  >
                    {isProcessing ? (
                      <RefreshCw className="size-4 animate-spin" />
                    ) : (
                      <Merge className="size-4" />
                    )}
                    {isProcessing ? "Merging..." : "Merge PDFs"}
                  </Button>
                </>
              )}

              {activeTab === "split" && (
                <>
                  <div className="space-y-3">
                    <Label>
                      Extraction Mode
                    </Label>
                    <SegmentedControl
                      fullWidth
                      variant="dark"
                      value={splitMode}
                      onValueChange={(value) =>
                        setSplitMode(value as "every" | "range")
                      }
                      className="grid grid-cols-2 gap-2 border-none bg-transparent p-0"
                      optionClassName="rounded-xl border px-3 py-2 text-xs"
                      options={[
                        { label: "Every Page (Zip)", value: "every" },
                        { label: "Custom Range", value: "range" },
                      ]}
                    />
                  </div>

                  {splitMode === "range" && (
                    <div className="space-y-3">
                      <Label className="text-muted-foreground">
                        Pages Extract Pattern
                      </Label>
                      <Input
                        value={splitRange}
                        onChange={(e) => setSplitRange(e.target.value)}
                        placeholder="e.g. 1-3, 5, 8-10"
                        className="bg-background/50 text-sm font-mono h-9"
                      />
                      <p className="text-[11px] text-muted-foreground mt-1">
                        Select specific pages to extract into a single new PDF
                        document.
                      </p>
                    </div>
                  )}

                  <Button
                    onClick={executeSplit}
                    disabled={
                      !splitFile ||
                      isProcessing ||
                      (splitMode === "range" && !splitRange)
                    }
                    className="w-full gap-2 h-12 rounded-xl shadow-lg font-semibold btn-accent"
                  >
                    {isProcessing ? (
                      <RefreshCw className="size-4 animate-spin" />
                    ) : (
                      <SplitSquareHorizontal className="size-4" />
                    )}
                    {isProcessing ? "Splitting..." : "Extract Pages"}
                  </Button>
                </>
              )}

              {activeTab === "compress" && (
                <>
                  <div className="flex items-start gap-3 p-3 rounded-xl bg-orange-500/10 border border-orange-500/20 text-orange-400">
                    <ImageIcon className="size-5 shrink-0 mt-0.5" />
                    <p className="text-[11px] leading-relaxed">
                      <strong>Note:</strong> Client-side compression flattens
                      text & vectors into optimized JPEG images to forcefully
                      reduce file size safely.
                    </p>
                  </div>

                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <Label>
                        Rendering Scale
                      </Label>
                      <Badge variant="outline" className="font-mono">
                        {compressResolution.toFixed(1)}x
                      </Badge>
                    </div>
                    <Slider
                      min={0.5}
                      max={3.0}
                      step={0.1}
                      value={compressResolution}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        setCompressResolution(Number(e.target.value))
                      }
                    />
                  </div>

                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <Label>
                        JPEG Image Quality
                      </Label>
                      <Badge variant="outline" className="font-mono">
                        {compressQuality}%
                      </Badge>
                    </div>
                    <Slider
                      min={10}
                      max={100}
                      step={5}
                      value={compressQuality}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        setCompressQuality(Number(e.target.value))
                      }
                    />
                  </div>

                  <Button
                    onClick={executeCompress}
                    disabled={!compressFile || isProcessing}
                    className="w-full gap-2 h-12 rounded-xl shadow-lg font-semibold btn-accent"
                  >
                    {isProcessing ? (
                      <RefreshCw className="size-4 animate-spin" />
                    ) : (
                      <Minimize2 className="size-4" />
                    )}
                    {isProcessing ? "Compressing..." : "Compress PDF"}
                  </Button>
                </>
              )}

              {activeTab === "reorder" && (
                <>
                  <p className="text-sm text-muted-foreground leading-relaxed">
                    Drag the page thumbnails in the main window to build a new
                    sequence, then click below to construct the new PDF.
                  </p>
                  <Button
                    onClick={executeReorder}
                    disabled={
                      !reorderFile || isProcessing || reorderPages.length === 0
                    }
                    className="w-full gap-2 h-12 rounded-xl shadow-lg font-semibold btn-accent"
                  >
                    {isProcessing ? (
                      <RefreshCw className="size-4 animate-spin" />
                    ) : (
                      <Download className="size-4" />
                    )}
                    {isProcessing ? "Processing..." : "Export Reordered PDF"}
                  </Button>
                </>
              )}

              {isProcessing && progress > 0 && progress < 100 && (
                <div className="space-y-2 pt-2">
                  <div className="flex text-xs justify-between mb-1">
                    <span className="text-muted-foreground">Processing...</span>
                    <span className="font-medium">{progress}%</span>
                  </div>
                  <Progress value={progress} className="h-2" />
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
