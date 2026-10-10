"use client";

import { getToolBySlug } from "@/config/tools";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import React, { useCallback, useState } from "react";
import {
  AlertCircle,
  ArchiveIcon,
  CheckCircle2,
  FileImage,
  Info,
  Loader2,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Separator } from "@/components/ui/layout/Separator";
import {
  type ConversionStatus,
  type FileItem,
  areFormatsEquivalent,
  detectFormat,
  FORMAT_LABELS,
  FORMAT_MIME_MAP,
  generateId,
  getDefaultTarget,
  MAX_FILE_SIZE,
  MAX_IMAGE_DIMENSION,
  MAX_IMAGE_PIXELS,
  SUPPORTED_OUTPUT_FORMATS,
} from "@/lib/converters/image-converter/converter";
import {
  convertImage,
  getImageMetadata,
} from "@/lib/converters/image-converter/engine";
import { Tooltip } from "@/components/ui/feedback/Tooltip";
import { ImageConverterFileCard } from "@/components/tools/image-converter/ImageConverterFileCard";
import {
  ACCEPTED_INPUT,
  UNSUPPORTED_OUTPUT_FORMATS,
} from "@/lib/tools/image-converter/constants";
import {
  formatMegapixels,
  summarizeUploadIssues,
} from "@/lib/tools/image-converter/utils";

const tool = getToolBySlug("image-converter");

export default function ImageConverterPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ImageConverter />
    </ToolPageShell>
  );
}


function ImageConverter() {
  const [files, setFiles] = useState<FileItem[]>([]);
  const [isDownloadingAll, setIsDownloadingAll] = useState(false);
  const [uploadWarning, setUploadWarning] = useState<string | null>(null);

  const addFiles = useCallback(async (newFiles: File[]) => {
    const validFiles: File[] = [];
    const rejectedFiles: string[] = [];

    for (const file of newFiles) {
      if (file.size > MAX_FILE_SIZE) {
        rejectedFiles.push(`${file.name} exceeds the 10MB upload limit`);
        continue;
      }

      try {
        const metadata = await getImageMetadata(file);
        const pixelCount = metadata.width * metadata.height;

        if (
          metadata.width > MAX_IMAGE_DIMENSION ||
          metadata.height > MAX_IMAGE_DIMENSION
        ) {
          rejectedFiles.push(
            `${file.name} is ${metadata.width}x${metadata.height}; max is ${MAX_IMAGE_DIMENSION}px per side`,
          );
          continue;
        }

        if (pixelCount > MAX_IMAGE_PIXELS) {
          rejectedFiles.push(
            `${file.name} is ${formatMegapixels(pixelCount)}; max is ${formatMegapixels(
              MAX_IMAGE_PIXELS,
            )}`,
          );
          continue;
        }

        validFiles.push(file);
      } catch {
        rejectedFiles.push(`Couldn't read dimensions for ${file.name}`);
      }
    }

    if (rejectedFiles.length > 0) {
      setUploadWarning(summarizeUploadIssues(rejectedFiles));
      setTimeout(() => setUploadWarning(null), 7000);
    }

    if (validFiles.length === 0) return;

    const items: FileItem[] = validFiles.map((file) => {
      const sourceFormat = detectFormat(file);
      const targetFormat = getDefaultTarget(sourceFormat);

      return {
        id: generateId(),
        file,
        name: file.name,
        size: file.size,
        type: file.type,
        sourceFormat,
        targetFormat,
        quality: 85,
        status: "idle" as ConversionStatus,
        progress: 0,
        preview: URL.createObjectURL(file),
      };
    });

    setFiles((prev) => [...prev, ...items]);
  }, []);

  const updateFile = useCallback((id: string, updates: Partial<FileItem>) => {
    setFiles((prev) =>
      prev.map((file) => (file.id === id ? { ...file, ...updates } : file)),
    );
  }, []);

  const revokeFileUrls = useCallback((file: FileItem) => {
    if (file.preview) URL.revokeObjectURL(file.preview);
    if (file.convertedUrl) URL.revokeObjectURL(file.convertedUrl);
  }, []);

  const removeFile = useCallback(
    (id: string) => {
      setFiles((prev) => {
        const file = prev.find((item) => item.id === id);
        if (file) {
          revokeFileUrls(file);
        }
        return prev.filter((item) => item.id !== id);
      });
    },
    [revokeFileUrls],
  );

  const clearAll = useCallback(() => {
    files.forEach(revokeFileUrls);
    setFiles([]);
  }, [files, revokeFileUrls]);

  const convertFile = useCallback(
    async (id: string) => {
      const file = files.find((item) => item.id === id);
      if (!file || file.status === "converting") return;

      updateFile(id, { status: "converting", progress: 0, error: undefined });

      const animateProgress = (
        from: number,
        to: number,
        duration: number,
      ): Promise<void> =>
        new Promise((resolve) => {
          const startTime = Date.now();

          const tick = () => {
            const elapsed = Date.now() - startTime;
            const progress = Math.min(elapsed / duration, 1);
            const current = Math.round(from + (to - from) * progress);
            updateFile(id, { progress: current });

            // Timers, not animation frames: the conversion awaits this
            // promise, and frames never fire in a background tab.
            if (progress < 1) {
              setTimeout(tick, 16);
            } else {
              resolve();
            }
          };

          tick();
        });

      try {
        if (file.sourceFormat === "unknown") {
          throw new Error("Unsupported source format");
        }

        if (!SUPPORTED_OUTPUT_FORMATS.includes(file.targetFormat)) {
          throw new Error(
            `${FORMAT_LABELS[file.targetFormat]} export is not available yet.`,
          );
        }

        const progressPromise = animateProgress(0, 85, 800);

        const blob = areFormatsEquivalent(file.sourceFormat, file.targetFormat)
          ? file.file.slice(
              0,
              file.file.size,
              FORMAT_MIME_MAP[file.targetFormat],
            )
          : await convertImage(file.file, file.targetFormat, file.quality);

        await progressPromise;
        await animateProgress(85, 100, 150);

        const url = URL.createObjectURL(blob);

        updateFile(id, {
          status: "done",
          progress: 100,
          convertedBlob: blob,
          convertedUrl: url,
        });
      } catch (error) {
        updateFile(id, {
          status: "error",
          progress: 0,
          error: error instanceof Error ? error.message : "Conversion failed",
        });
      }
    },
    [files, updateFile],
  );

  const convertAll = useCallback(async () => {
    const toConvert = files.filter(
      (file) =>
        (file.status === "idle" || file.status === "error") &&
        SUPPORTED_OUTPUT_FORMATS.includes(file.targetFormat),
    );

    for (const file of toConvert) {
      await convertFile(file.id);
    }
  }, [files, convertFile]);

  const downloadFile = useCallback((item: FileItem) => {
    if (!item.convertedUrl) return;

    const anchor = document.createElement("a");
    anchor.href = item.convertedUrl;
    const baseName = item.name.replace(/\.[^/.]+$/, "");
    anchor.download = `${baseName}.${item.targetFormat}`;
    anchor.click();
  }, []);

  const downloadAll = useCallback(async () => {
    const doneFiles = files.filter(
      (file) => file.status === "done" && file.convertedBlob,
    );
    if (doneFiles.length === 0) return;

    setIsDownloadingAll(true);

    try {
      const JSZip = (await import("jszip")).default;
      const zip = new JSZip();

      for (const item of doneFiles) {
        const baseName = item.name.replace(/\.[^/.]+$/, "");
        if (item.convertedBlob) {
          zip.file(`${baseName}.${item.targetFormat}`, item.convertedBlob);
        }
      }

      const content = await zip.generateAsync({ type: "blob" });
      const url = URL.createObjectURL(content);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "converted-images.zip";
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      console.error("Failed to create ZIP:", error);
    }

    setIsDownloadingAll(false);
  }, [files]);

  const hasFiles = files.length > 0;
  const doneCount = files.filter((file) => file.status === "done").length;
  const convertibleCount = files.filter(
    (file) =>
      (file.status === "idle" || file.status === "error") &&
      SUPPORTED_OUTPUT_FORMATS.includes(file.targetFormat),
  ).length;
  const isConverting = files.some((file) => file.status === "converting");

  return (
    <div className="space-y-6">
      {UNSUPPORTED_OUTPUT_FORMATS.length > 0 && (
        <div className="mx-auto mb-4 max-w-4xl rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3">
          <div className="flex items-start gap-2.5">
            <AlertCircle className="mt-0.5 size-4 shrink-0 text-amber-500" />
            <div>
              <p className="text-sm font-medium text-amber-600 dark:text-amber-400">
                HEIF Export Limitation
              </p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground sm:text-sm">
                HEIF and HEIC files can be uploaded as sources, but the bundled
                codec does not yet export
                <strong> HEIF </strong>
                output. PNG, JPG, JPEG, WebP, AVIF, TIFF, and ICO are available
                as targets.
              </p>
            </div>
          </div>
        </div>
      )}

      <div className="mx-auto mb-4 max-w-4xl rounded-lg border border-blue-500/20 bg-blue-500/5 px-4 py-3">
        <div className="flex items-start gap-2.5">
          <Info className="mt-0.5 size-4 shrink-0 text-blue-500" />
          <div>
            <p className="text-sm font-medium text-blue-600 dark:text-blue-400">
              Upload Limits
            </p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground sm:text-sm">
              Uploads are limited to 10MB, 4096px on either side, and 20MP
              total. ICO exports above 512px are automatically resized to fit
              the encoder limit instead of failing.
            </p>
          </div>
        </div>
      </div>

      {uploadWarning && (
        <div className="mb-6 rounded-lg border border-red-500/30 bg-red-500/5 p-4">
          <div className="flex items-start gap-3">
            <AlertCircle className="mt-0.5 size-5 shrink-0 text-red-500" />
            <div className="flex-1">
              <p className="font-medium text-red-600 dark:text-red-400">
                Upload Rejected
              </p>
              <p className="mt-1 text-sm text-muted-foreground">
                {uploadWarning}
              </p>
            </div>
            <Button
              onClick={() => setUploadWarning(null)}
              variant="ghost"
              size="icon"
              className="h-6 w-6 text-red-500 hover:text-red-600 dark:hover:text-red-400"
            >
              <X className="size-4" />
            </Button>
          </div>
        </div>
      )}

      <FileDropZoneCard
        fileTypeLabel="image files"
        supportedFormats="PNG, JPG, JPEG, WebP, AVIF, TIFF, HEIF, and ICO"
        accept={ACCEPTED_INPUT}
        multiple
        onFilesSelected={(incoming) => {
          void addFiles(incoming);
        }}
      />

      {hasFiles && (
        <div className="mt-8 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-3">
              <Badge variant="secondary" className="px-3 py-1 text-sm">
                <FileImage className="mr-1 size-3.5" />
                {files.length} file{files.length !== 1 ? "s" : ""}
              </Badge>
              {doneCount > 0 && (
                <Badge
                  variant="outline"
                  className="border-white/15 px-3 py-1 text-sm text-white/75"
                >
                  <CheckCircle2 className="mr-1 size-3.5" />
                  {doneCount} converted
                </Badge>
              )}
            </div>

            <div className="flex items-center gap-2">
              {convertibleCount > 0 && (
                <Button
                  onClick={convertAll}
                  disabled={isConverting}
                  size="sm"
                  className="gap-2"
                >
                  {isConverting ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Sparkles className="size-4" />
                  )}
                  Convert All
                </Button>
              )}
              {doneCount > 1 && (
                <Button
                  onClick={downloadAll}
                  variant="outline"
                  size="sm"
                  className="gap-2"
                  disabled={isDownloadingAll}
                >
                  {isDownloadingAll ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <ArchiveIcon className="size-4" />
                  )}
                  Download All (.zip)
                </Button>
              )}
              <Separator orientation="vertical" className="h-6" />
              <Tooltip content="Clear all files">
                <Button
                  onClick={clearAll}
                  variant="ghost"
                  size="icon"
                  className="text-muted-foreground hover:text-destructive"
                >
                  <Trash2 className="size-4" />
                </Button>
              </Tooltip>
            </div>
          </div>

          <div className="space-y-3">
            {files.map((item) => (
              <ImageConverterFileCard
                key={item.id}
                item={item}
                onUpdateTarget={(target) =>
                  updateFile(item.id, {
                    targetFormat: target,
                    status: "idle",
                    progress: 0,
                    convertedBlob: undefined,
                    convertedUrl: undefined,
                    error: undefined,
                  })
                }
                onUpdateQuality={(quality) => updateFile(item.id, { quality })}
                onConvert={() => convertFile(item.id)}
                onDownload={() => downloadFile(item)}
                onRemove={() => removeFile(item.id)}
                onRetry={() => {
                  updateFile(item.id, {
                    status: "idle",
                    progress: 0,
                    error: undefined,
                  });
                  convertFile(item.id);
                }}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

