// ImageConverterFileCard.tsx
// Card component for managing and rendering individual image conversion operations.

"use client";

import {
  AlertCircle,
  ArrowRight,
  Download,
  ImageIcon,
  Info,
  Loader2,
  RotateCcw,
  Sparkles,
  X,
} from "lucide-react";
import React, { useState } from "react";
import {
  ToolInputCard,
  ToolInputCardBadge,
  ToolInputCardButton,
  ToolInputCardDismissButton,
  ToolInputCardField,
  ToolInputCardGrid,
  ToolInputCardHeader,
  ToolInputCardInner,
  ToolInputCardLabel,
  ToolInputCardProgress,
  ToolInputCardSelect,
  ToolInputCardSlider,
  ToolInputCardText,
  ToolInputCardTitle,
} from "@/components/common/ToolInputCard";
import { Tooltip } from "@/components/ui/feedback/Tooltip";
import {
  type FileItem,
  FORMAT_LABELS,
  formatFileSize,
  getAvailableTargets,
  type ImageFormat,
  LOSSY_FORMATS,
} from "@/lib/converters/image-converter/converter";
import { UNSUPPORTED_OUTPUT_SET } from "@/lib/tools/image-converter/constants";

interface ImageConverterFileCardProps {
  item: FileItem;
  onUpdateTarget: (target: ImageFormat) => void;
  onUpdateQuality: (quality: number) => void;
  onConvert: () => void;
  onDownload: () => void;
  onRemove: () => void;
  onRetry: () => void;
}

function getStatusBadgeProps(status: FileItem["status"]) {
  switch (status) {
    case "idle":
      return {
        label: "Ready",
        variant: "outline" as const,
        className: "px-1.5 py-0 text-[10px]",
      };
    case "converting":
      return {
        label: "Converting",
        variant: "secondary" as const,
        className:
          "border-blue-500/20 bg-blue-500/10 px-1.5 py-0 text-[10px] text-blue-600 dark:text-blue-400",
      };
    case "done":
      return {
        label: "Done",
        variant: "secondary" as const,
        className:
          "border-white/15 bg-white/[0.06] px-1.5 py-0 text-[10px] text-white/75",
      };
    case "error":
      return {
        label: "Error",
        variant: "destructive" as const,
        className: "px-1.5 py-0 text-[10px]",
      };
  }
}

export function ImageConverterFileCard({
  item,
  onUpdateTarget,
  onUpdateQuality,
  onConvert,
  onDownload,
  onRemove,
  onRetry,
}: ImageConverterFileCardProps) {
  const [failedPreview, setFailedPreview] = useState<string | null>(null);

  const availableTargets = getAvailableTargets(item.sourceFormat);
  const isLossyTarget = LOSSY_FORMATS.includes(item.targetFormat);
  const isTargetSupported = !UNSUPPORTED_OUTPUT_SET.has(item.targetFormat);
  const convertDisabled =
    item.status === "converting" ||
    !isTargetSupported ||
    item.sourceFormat === "unknown";
  const showPreview = Boolean(item.preview) && failedPreview !== item.preview;
  const statusBadge = getStatusBadgeProps(item.status);
  const tone =
    item.status === "converting"
      ? "converting"
      : item.status === "done"
        ? "done"
        : item.status === "error"
          ? "error"
          : "idle";

  return (
    <ToolInputCard
      tone={tone}
      className="transition-all duration-200 hover:shadow-md"
    >
      <ToolInputCardInner className="space-y-4 p-4 pl-5">
        <ToolInputCardHeader className="sm:flex-nowrap sm:items-center">
          <div className="flex min-w-0 flex-1 items-center gap-3">
            <div className="relative flex size-14 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-white/10 bg-muted/50">
              {showPreview ? (
                /* eslint-disable-next-line @next/next/no-img-element -- blob URLs from createObjectURL don't work with next/image */
                <img
                  src={item.preview}
                  alt={item.name}
                  className="size-full object-cover"
                  onError={() => setFailedPreview(item.preview ?? null)}
                />
              ) : (
                <ImageIcon className="size-6 text-muted-foreground" />
              )}
            </div>

            <div className="min-w-0 flex-1">
              <ToolInputCardTitle
                className="truncate text-base"
                title={item.name}
              >
                {item.name}
              </ToolInputCardTitle>
              <div className="mt-1 flex flex-wrap items-center gap-2">
                <ToolInputCardText className="text-xs">
                  {formatFileSize(item.size)}
                </ToolInputCardText>
                <ToolInputCardBadge className="px-1.5 py-0 text-[10px]">
                  {item.sourceFormat === "unknown"
                    ? "?"
                    : FORMAT_LABELS[item.sourceFormat]}
                </ToolInputCardBadge>
                <ArrowRight className="size-3 text-muted-foreground" />
                <ToolInputCardBadge
                  variant="secondary"
                  className="px-1.5 py-0 text-[10px] font-bold"
                >
                  {FORMAT_LABELS[item.targetFormat]}
                </ToolInputCardBadge>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {item.status === "done" && (
              <ToolInputCardBadge>{item.status}</ToolInputCardBadge>
            )}
            <Tooltip content="Remove file">
              <ToolInputCardDismissButton
                onClick={onRemove}
                className="opacity-100 sm:opacity-0 sm:transition-opacity sm:group-hover:opacity-100 hover:text-destructive"
              >
                <X className="size-3.5" />
              </ToolInputCardDismissButton>
            </Tooltip>
          </div>
        </ToolInputCardHeader>

        <ToolInputCardGrid className="items-start lg:grid-cols-[minmax(0,1.1fr)_minmax(0,0.9fr)_minmax(0,1fr)]">
          <ToolInputCardField>
            <ToolInputCardLabel>Format</ToolInputCardLabel>
            <ToolInputCardSelect
              options={availableTargets.map((format) => ({
                label: `${FORMAT_LABELS[format]}${UNSUPPORTED_OUTPUT_SET.has(format) ? " (Unavailable)" : ""}`,
                value: format,
                disabled: UNSUPPORTED_OUTPUT_SET.has(format),
              }))}
              value={item.targetFormat}
              onChange={(e) => onUpdateTarget(e.target.value as ImageFormat)}
              disabled={item.status === "converting"}
              className="h-10 text-xs"
            />
          </ToolInputCardField>

          <ToolInputCardField>
            <ToolInputCardLabel>Quality</ToolInputCardLabel>
            {isLossyTarget ? (
              <div className="flex min-w-[120px] items-center gap-2">
                <Tooltip content={`Quality: ${item.quality}%`}>
                  <span className="w-8 whitespace-nowrap text-right text-xs text-muted-foreground">
                    {item.quality}%
                  </span>
                </Tooltip>
                <ToolInputCardSlider
                  min={10}
                  max={100}
                  step={5}
                  value={item.quality}
                  onChange={(e) =>
                    onUpdateQuality(parseInt(e.target.value, 10))
                  }
                  disabled={item.status === "converting"}
                  className="w-20"
                />
              </div>
            ) : (
              <ToolInputCardText className="text-xs">
                No quality control for this format
              </ToolInputCardText>
            )}
          </ToolInputCardField>

          <ToolInputCardField>
            <ToolInputCardLabel>Actions</ToolInputCardLabel>
            <div className="flex flex-wrap items-center gap-2">
              <ToolInputCardBadge
                variant={statusBadge.variant}
                className={statusBadge.className}
              >
                {statusBadge.label}
              </ToolInputCardBadge>
              {item.status === "idle" && (
                <Tooltip
                  content={
                    !isTargetSupported
                      ? "This output format is unavailable"
                      : item.sourceFormat === "unknown"
                        ? "Unsupported source format"
                        : "Convert this file"
                  }
                >
                  <ToolInputCardButton
                    onClick={onConvert}
                    size="sm"
                    variant={isTargetSupported ? "default" : "outline"}
                    className="gap-1.5 text-xs"
                    disabled={convertDisabled}
                  >
                    <Sparkles className="size-3" />
                    Convert
                  </ToolInputCardButton>
                </Tooltip>
              )}

              {item.status === "converting" && (
                <span className="inline-flex items-center gap-1.5 text-xs text-blue-600 dark:text-blue-400">
                  <Loader2 className="size-3.5 animate-spin" />
                  {item.progress}%
                </span>
              )}

              {item.status === "done" && (
                <ToolInputCardButton
                  onClick={onDownload}
                  variant="outline"
                  size="sm"
                  className="gap-1.5 border-white/15 text-xs text-white/75 hover:bg-white/[0.06]"
                >
                  <Download className="size-3" />
                  Download
                  {item.convertedBlob
                    ? ` (${formatFileSize(item.convertedBlob.size)})`
                    : ""}
                </ToolInputCardButton>
              )}

              {item.status === "error" && (
                <Tooltip content="Retry conversion">
                  <ToolInputCardButton
                    onClick={onRetry}
                    variant="outline"
                    size="icon"
                    className="size-8 border-red-500/30 px-0 text-red-500 hover:bg-red-500/10"
                  >
                    <RotateCcw className="size-3.5" />
                  </ToolInputCardButton>
                </Tooltip>
              )}
            </div>
          </ToolInputCardField>
        </ToolInputCardGrid>

        {item.status === "converting" && (
          <div className="mt-3">
            <ToolInputCardProgress value={item.progress} className="h-1.5" />
          </div>
        )}

        {item.status === "error" && item.error && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-red-500/20 bg-red-500/5 p-2.5">
            <AlertCircle className="mt-0.5 size-3.5 shrink-0 text-red-500" />
            <p className="text-xs text-red-600 dark:text-red-400">
              {item.error}
            </p>
          </div>
        )}

        {!isTargetSupported && item.status === "idle" && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-amber-500/20 bg-amber-500/5 p-2.5">
            <Info className="mt-0.5 size-3.5 shrink-0 text-amber-500" />
            <p className="text-xs text-amber-600 dark:text-amber-400">
              HEIF files can be used as input, but HEIF output is not available
              in the bundled codec yet.
            </p>
          </div>
        )}
      </ToolInputCardInner>
    </ToolInputCard>
  );
}
