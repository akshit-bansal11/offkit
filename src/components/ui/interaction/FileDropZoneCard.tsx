// FileDropZoneCard.tsx
// Interactive card wrapper with drag-and-drop file upload capabilities.

"use client";

import { FileUp, Upload } from "lucide-react";
import React, { useCallback, useRef, useState } from "react";
import { Button } from "@/components/ui/interaction/Button";
import { cn } from "@/lib/utils";

interface FileDropZoneCardProps {
  fileTypeLabel: string;
  supportedFormats: string;
  accept: string;
  title?: string;
  description?: string;
  multiple?: boolean;
  disabled?: boolean;
  className?: string;
  children?: React.ReactNode;
  onFilesSelected: (files: File[]) => void;
}

export function FileDropZoneCard({
  fileTypeLabel,
  supportedFormats,
  accept,
  title,
  description,
  multiple = false,
  disabled = false,
  className,
  children,
  onFilesSelected,
}: FileDropZoneCardProps) {
  const [isDragging, setIsDragging] = useState(false);
  const dragCounter = useRef(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const openPicker = useCallback(() => {
    if (disabled) return;
    fileInputRef.current?.click();
  }, [disabled]);

  const resetDragState = useCallback(() => {
    dragCounter.current = 0;
    setIsDragging(false);
  }, []);

  const handleDragEnter = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (disabled) return;
      event.preventDefault();
      event.stopPropagation();
      dragCounter.current += 1;
      setIsDragging(true);
    },
    [disabled],
  );

  const handleDragLeave = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (disabled) return;
      event.preventDefault();
      event.stopPropagation();
      dragCounter.current = Math.max(0, dragCounter.current - 1);
      if (dragCounter.current === 0) setIsDragging(false);
    },
    [disabled],
  );

  const handleDragOver = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (disabled) return;
      event.preventDefault();
      event.stopPropagation();
    },
    [disabled],
  );

  const handleDrop = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (disabled) return;
      event.preventDefault();
      event.stopPropagation();
      const files = Array.from(event.dataTransfer.files);
      resetDragState();
      if (files.length === 0) return;
      onFilesSelected(multiple ? files : [files[0]]);
    },
    [disabled, multiple, onFilesSelected, resetDragState],
  );

  const handleFileChange = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      if (disabled) return;
      const files = event.target.files ? Array.from(event.target.files) : [];
      event.target.value = "";
      if (files.length === 0) return;
      onFilesSelected(multiple ? files : [files[0]]);
    },
    [disabled, multiple, onFilesSelected],
  );

  return (
    <div
      className={cn(
        "rounded-2xl border border-white/[0.06] bg-white/[0.02] backdrop-blur-sm p-5 space-y-4",
        className,
      )}
    >
      {title || description ? (
        <div className="space-y-1">
          {title ? (
            <p className="text-lg font-semibold text-foreground">{title}</p>
          ) : null}
          {description ? (
            <p className="text-sm text-muted-foreground">{description}</p>
          ) : null}
        </div>
      ) : null}

      <div
        role="button"
        tabIndex={disabled ? -1 : 0}
        aria-disabled={disabled}
        className={cn(
          "drop-zone group relative flex min-h-[240px] w-full flex-col items-center justify-center gap-4 outline-none transition-all duration-300 focus-visible:ring-2 focus-visible:ring-ring motion-reduce:transition-none",
          disabled ? "cursor-not-allowed opacity-60" : "cursor-pointer",
          isDragging && "drop-zone-active scale-[1.01]",
        )}
        style={
          isDragging
            ? {
                borderColor: "rgba(255,255,255,0.25)",
                boxShadow: "0 0 40px rgba(255,255,255,0.06)",
              }
            : undefined
        }
        onClick={openPicker}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) return;
          if (disabled) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            openPicker();
          }
        }}
        onDragEnter={handleDragEnter}
        onDragLeave={handleDragLeave}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
      >
        <div
          className={cn("rounded-full border p-4 transition-all duration-300")}
          style={{
            borderColor: isDragging
              ? "rgba(255,255,255,0.2)"
              : "rgba(255,255,255,0.08)",
            backgroundColor: isDragging
              ? "rgba(255,255,255,0.06)"
              : "rgba(255,255,255,0.03)",
          }}
        >
          <FileUp
            className={cn("size-8 transition-colors duration-300")}
            style={{
              color: isDragging
                ? "rgba(255,255,255,0.9)"
                : "rgba(255,255,255,0.4)",
            }}
          />
        </div>

        <div className="space-y-1">
          <p className="text-lg font-semibold text-foreground">
            Drop {fileTypeLabel} here
          </p>
          <p className="text-sm text-muted-foreground">
            Supports {supportedFormats}.
          </p>
        </div>

        <input
          ref={fileInputRef}
          type="file"
          accept={accept}
          multiple={multiple}
          className="hidden"
          onChange={handleFileChange}
          disabled={disabled}
        />

        <Button
          type="button"
          variant="secondary"
          className="bg-white/[0.08] text-foreground hover:bg-white/[0.12]"
          disabled={disabled}
          onClick={(event) => {
            event.stopPropagation();
            openPicker();
          }}
        >
          <Upload className="size-4" />
          Upload {fileTypeLabel}
        </Button>
      </div>

      {children ? <div className="pt-4">{children}</div> : null}
    </div>
  );
}
