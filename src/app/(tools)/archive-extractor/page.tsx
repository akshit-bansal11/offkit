"use client";

import JSZip from "jszip";
import {
  ChevronDown,
  ChevronRight,
  Download,
  File,
  Folder,
  FolderOpen,
  Loader2,
  X,
} from "lucide-react";
import React, { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("archive-extractor");

export default function ArchiveExtractorPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ArchiveExtractorTool />
    </ToolPageShell>
  );
}

interface FileTreeNode {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  children: FileTreeNode[];
}

function buildFileTree(zip: JSZip): FileTreeNode[] {
  const root: FileTreeNode = {
    name: "/",
    path: "",
    isDir: true,
    size: 0,
    children: [],
  };

  zip.forEach((relativePath, entry) => {
    const parts = relativePath.split("/").filter(Boolean);
    let current = root;
    for (let i = 0; i < parts.length; i++) {
      const isLast = i === parts.length - 1;
      const existing = current.children.find((c) => c.name === parts[i]);
      if (existing) {
        current = existing;
      } else {
        const node: FileTreeNode = {
          name: parts[i],
          path: relativePath,
          isDir: !isLast || entry.dir,
          // @ts-expect-error — _data is internal
          size: isLast && !entry.dir ? (entry._data?.uncompressedSize ?? 0) : 0,
          children: [],
        };
        current.children.push(node);
        current = node;
      }
    }
  });

  // Sort: dirs first, then files
  const sortNodes = (nodes: FileTreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.isDir === b.isDir) return a.name.localeCompare(b.name);
      return a.isDir ? -1 : 1;
    });
    nodes.forEach((n) => sortNodes(n.children));
  };
  sortNodes(root.children);

  return root.children;
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface TreeNodeProps {
  node: FileTreeNode;
  depth: number;
  zip: JSZip;
  onSelect: (path: string, checked: boolean) => void;
  selected: Set<string>;
}

function TreeNode({ node, depth, zip, onSelect, selected }: TreeNodeProps) {
  const [expanded, setExpanded] = useState(depth < 2);

  const downloadFile = async (path: string) => {
    const entry = zip.file(path);
    if (!entry) return;
    const blob = await entry.async("blob");
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = node.name;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div>
      <div
        className={`group flex items-center gap-1.5 rounded-lg px-2 py-1 text-sm hover:bg-white/[0.04] transition-colors`}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        {node.isDir ? (
          <button
            onClick={() => setExpanded(!expanded)}
            className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
          >
            {expanded ? (
              <ChevronDown className="size-3.5" />
            ) : (
              <ChevronRight className="size-3.5" />
            )}
            {expanded ? (
              <FolderOpen className="size-3.5 text-warning" />
            ) : (
              <Folder className="size-3.5 text-warning" />
            )}
          </button>
        ) : (
          <>
            <input
              type="checkbox"
              checked={selected.has(node.path)}
              onChange={(e) => onSelect(node.path, e.target.checked)}
              className="size-3 accent-success"
            />
            <File className="size-3.5 text-muted-foreground" />
          </>
        )}

        <span
          className={`flex-1 truncate text-xs ${node.isDir ? "font-medium text-foreground" : "text-muted-foreground"}`}
        >
          {node.name}
        </span>

        {!node.isDir && node.size > 0 && (
          <span className="text-[10px] text-muted-foreground/60 shrink-0">
            {formatFileSize(node.size)}
          </span>
        )}

        {!node.isDir && (
          <button
            onClick={() => void downloadFile(node.path)}
            className="opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:text-foreground"
            title="Download"
          >
            <Download className="size-3.5" />
          </button>
        )}
      </div>

      {node.isDir && expanded && node.children.length > 0 && (
        <div>
          {node.children.map((child, i) => (
            <TreeNode
              key={i}
              node={child}
              depth={depth + 1}
              zip={zip}
              onSelect={onSelect}
              selected={selected}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ArchiveExtractorTool() {
  const [file, setFile] = useState<File | null>(null);
  const [zip, setZip] = useState<JSZip | null>(null);
  const [tree, setTree] = useState<FileTreeNode[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [isExtracting, setIsExtracting] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [totalFiles, setTotalFiles] = useState(0);

  const handleFile = async (f: File) => {
    setFile(f);
    setError(null);
    setSelected(new Set());
    setIsLoading(true);

    try {
      const z = await JSZip.loadAsync(f);
      const t = buildFileTree(z);
      setZip(z);
      setTree(t);

      let fc = 0;
      z.forEach((_, entry) => {
        if (!entry.dir) fc++;
      });
      setTotalFiles(fc);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to read ZIP file.");
    }
    setIsLoading(false);
  };

  const handleSelect = (path: string, checked: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (checked) next.add(path);
      else next.delete(path);
      return next;
    });
  };

  const extractAll = async () => {
    if (!zip || !file) return;
    setIsExtracting(true);
    setProgress(0);

    try {
      const JSZipNew = (await import("jszip")).default;
      const outZip = new JSZipNew();

      zip.forEach((path, entry) => {
        if (!entry.dir) {
          outZip.file(
            path,
            entry.async("blob").then((b) => b),
          );
        }
      });

      const blob = await outZip.generateAsync({ type: "blob" }, (m) =>
        setProgress(Math.round(m.percent)),
      );
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${file.name.replace(".zip", "")}_extracted.zip`;
      a.click();
      URL.revokeObjectURL(url);
      setProgress(100);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Extraction failed.");
    }
    setIsExtracting(false);
  };

  const extractSelected = async () => {
    if (!zip || selected.size === 0) return;
    setIsExtracting(true);
    setProgress(0);

    try {
      const JSZipNew = (await import("jszip")).default;
      const outZip = new JSZipNew();

      for (const path of selected) {
        const entry = zip.file(path);
        if (entry) {
          const blob = await entry.async("blob");
          outZip.file(path, blob);
        }
      }

      const blob = await outZip.generateAsync({ type: "blob" }, (m) =>
        setProgress(Math.round(m.percent)),
      );
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `selected_files.zip`;
      a.click();
      URL.revokeObjectURL(url);
      setProgress(100);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Extraction failed.");
    }
    setIsExtracting(false);
  };

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="a ZIP archive"
        supportedFormats="ZIP"
        accept=".zip,application/zip"
        multiple={false}
        onFilesSelected={(files) => {
          const f = files[0];
          if (f) void handleFile(f);
        }}
      />

      {/* TAR note */}
      <div className="flex items-center gap-2 rounded-2xl border border-white/[0.06] bg-white/[0.02] px-4 py-2.5 text-xs text-muted-foreground">
        <FolderOpen className="size-3.5 shrink-0" />
        Only ZIP format is supported. TAR/GZ support coming soon.
      </div>

      {isLoading && (
        <Card className="flex items-center justify-center gap-2 border-white/10 bg-background/30 py-8">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
          <p className="text-sm text-muted-foreground">Parsing archive…</p>
        </Card>
      )}

      {file && zip && tree.length > 0 && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          {/* Header */}
          <div className="flex flex-wrap items-center gap-3 px-5 py-3">
            <div className="flex items-center gap-2">
              <FolderOpen className="size-4 text-warning" />
              <span className="text-sm font-medium">{file.name}</span>
            </div>
            <Badge
              variant="outline"
              className="border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px]"
            >
              {totalFiles} files
            </Badge>
            <Badge
              variant="outline"
              className="border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px]"
            >
              {formatFileSize(file.size)}
            </Badge>
            <Button
              variant="ghost"
              size="icon"
              className="ml-auto size-7 text-muted-foreground"
              onClick={() => {
                setFile(null);
                setZip(null);
                setTree([]);
              }}
            >
              <X className="size-3.5" />
            </Button>
          </div>

          {/* Tree */}
          <div className="max-h-[500px] overflow-y-auto p-3">
            {tree.map((node, i) => (
              <TreeNode
                key={i}
                node={node}
                depth={0}
                zip={zip}
                onSelect={handleSelect}
                selected={selected}
              />
            ))}
          </div>

          {/* Actions */}
          <div className="flex flex-wrap items-center gap-3 px-5 py-3">
            {isExtracting && (
              <div className="flex flex-1 items-center gap-3">
                <Progress
                  value={progress}
                  className="h-1.5 flex-1 bg-white/10"
                />
                <span className="text-xs text-muted-foreground">
                  {progress}%
                </span>
              </div>
            )}

            {selected.size > 0 && !isExtracting && (
              <p className="text-xs text-muted-foreground flex-1">
                {selected.size} file{selected.size !== 1 ? "s" : ""} selected
              </p>
            )}

            <div className="ml-auto flex items-center gap-2">
              {selected.size > 0 && (
                <Button
                  variant="outline"
                  size="sm"
                  className="h-8 border-white/10 bg-white/[0.04] text-xs"
                  disabled={isExtracting}
                  onClick={() => void extractSelected()}
                >
                  {isExtracting ? (
                    <Loader2 className="size-3.5 animate-spin" />
                  ) : (
                    <Download className="size-3.5" />
                  )}
                  Extract Selected
                </Button>
              )}
              <Button
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={isExtracting}
                onClick={() => void extractAll()}
              >
                {isExtracting ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Download className="size-3.5" />
                )}
                Extract All
              </Button>
            </div>
          </div>
        </Card>
      )}

      {!file && !isLoading && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <FolderOpen className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Upload a ZIP archive to explore and extract files.
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
