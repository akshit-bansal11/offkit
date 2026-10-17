"use client";

import JSZip from "jszip";
import { Archive, FolderArchive, Loader2, X } from "lucide-react";
import React, { useCallback, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { Input } from "@/components/ui/form/Input";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("archiver");

export default function ArchiverPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ArchiverTool />
    </ToolPageShell>
  );
}

interface ArchiveFile {
  id: string;
  file: File;
  name: string;
  size: number;
  path: string;
}

function uid() {
  return Math.random().toString(36).slice(2, 9);
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getFileIcon(name: string) {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const icons: Record<string, string> = {
    pdf: "📄",
    jpg: "🖼️",
    jpeg: "🖼️",
    png: "🖼️",
    gif: "🖼️",
    webp: "🖼️",
    mp4: "🎬",
    webm: "🎬",
    mov: "🎬",
    mkv: "🎬",
    mp3: "🎵",
    wav: "🎵",
    flac: "🎵",
    js: "📜",
    ts: "📜",
    jsx: "📜",
    tsx: "📜",
    py: "📜",
    json: "📋",
    html: "🌐",
    css: "🎨",
    zip: "🗜️",
    tar: "🗜️",
    gz: "🗜️",
    txt: "📝",
    md: "📝",
  };
  return icons[ext] ?? "📁";
}

function ArchiverTool() {
  const [files, setFiles] = useState<ArchiveFile[]>([]);
  const [archiveName, setArchiveName] = useState("archive");
  const [compression, setCompression] = useState("6");
  const [isCreating, setIsCreating] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const addFiles = useCallback((incoming: File[]) => {
    const items: ArchiveFile[] = incoming.map((f) => ({
      id: uid(),
      file: f,
      name: f.name,
      size: f.size,
      path: f.name,
    }));
    setFiles((prev) => [...prev, ...items]);
  }, []);

  const removeFile = (id: string) => {
    setFiles((prev) => prev.filter((f) => f.id !== id));
  };

  const updatePath = (id: string, path: string) => {
    setFiles((prev) => prev.map((f) => (f.id === id ? { ...f, path } : f)));
  };

  const createZip = async () => {
    if (files.length === 0) return;
    setIsCreating(true);
    setProgress(0);
    setError(null);

    try {
      const zip = new JSZip();
      for (const item of files) {
        zip.file(item.path || item.name, item.file);
      }

      const blob = await zip.generateAsync(
        {
          type: "blob",
          compression: "DEFLATE",
          compressionOptions: {
            level: parseInt(compression, 10) as
              | 1
              | 2
              | 3
              | 4
              | 5
              | 6
              | 7
              | 8
              | 9,
          },
        },
        (meta) => setProgress(Math.round(meta.percent)),
      );

      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${archiveName || "archive"}.zip`;
      a.click();
      URL.revokeObjectURL(url);
      setProgress(100);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create ZIP.");
    }
    setIsCreating(false);
  };

  const totalSize = files.reduce((s, f) => s + f.size, 0);

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="any files"
        supportedFormats="All file types accepted"
        accept="*"
        multiple
        onFilesSelected={addFiles}
      />

      {files.length > 0 && (
        <Card className="border-white/10 bg-white/[0.015]">
          <div className="divide-y divide-white/[0.06]">
            {files.map((item) => (
              <div key={item.id} className="flex items-center gap-3 px-4 py-3">
                <span className="text-lg leading-none">
                  {getFileIcon(item.name)}
                </span>
                <div className="min-w-0 flex-1">
                  <Input
                    value={item.path}
                    onChange={(e) => updatePath(item.id, e.target.value)}
                    className="h-7 border-white/5 bg-transparent text-xs font-mono focus-visible:bg-white/[0.04] focus-visible:ring-0 px-1"
                    title="Edit the path inside the ZIP"
                  />
                </div>
                <Badge
                  variant="outline"
                  className="border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px] shrink-0"
                >
                  {formatFileSize(item.size)}
                </Badge>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7 shrink-0 text-muted-foreground"
                  onClick={() => removeFile(item.id)}
                  disabled={isCreating}
                >
                  <X className="size-3.5" />
                </Button>
              </div>
            ))}
          </div>

          <div className="border-t border-white/[0.06] p-4 text-xs text-muted-foreground">
            {files.length} file{files.length !== 1 ? "s" : ""} ·{" "}
            {formatFileSize(totalSize)} total
          </div>
        </Card>
      )}

      {files.length === 0 && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <FolderArchive className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Drop files here to add them to your archive.
          </p>
        </Card>
      )}

      <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
        <div className="grid gap-5 p-5 sm:grid-cols-3">
          <div className="space-y-2">
            <p className="text-sm font-medium">Archive name</p>
            <div className="flex items-center gap-1">
              <Input
                value={archiveName}
                onChange={(e) => setArchiveName(e.target.value)}
                placeholder="archive"
                className="h-9 border-white/10 bg-white/[0.04] text-sm"
              />
              <span className="shrink-0 text-xs text-muted-foreground">
                .zip
              </span>
            </div>
          </div>

          <div className="space-y-2">
            <p className="text-sm font-medium">Compression</p>
            <Select
              options={[
                { label: "None (level 0)", value: "0" },
                { label: "Fast (level 3)", value: "3" },
                { label: "Default (level 6)", value: "6" },
                { label: "Best (level 9)", value: "9" },
              ]}
              value={compression}
              onChange={(e) => setCompression(e.target.value)}
              className="h-9 border-white/10 bg-white/[0.04] text-sm"
            />
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-3 px-5 py-3">
          {isCreating && (
            <div className="flex flex-1 items-center gap-3">
              <Progress value={progress} className="h-1.5 flex-1 bg-white/10" />
              <span className="text-xs text-muted-foreground">{progress}%</span>
            </div>
          )}

          <div className="ml-auto">
            <Button
              size="sm"
              className="h-8 gap-1.5 text-xs"
              disabled={isCreating || files.length === 0}
              onClick={() => void createZip()}
            >
              {isCreating ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Archive className="size-3.5" />
              )}
              {isCreating ? "Creating…" : "Create ZIP"}
            </Button>
          </div>
        </div>
      </Card>

      {error && (
        <div className="rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-sm text-red-200">
          {error}
        </div>
      )}
    </div>
  );
}
