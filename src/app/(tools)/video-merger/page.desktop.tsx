"use client";

import { fetchFile } from "@ffmpeg/util";
import {
  AlertTriangle,
  Download,
  GripVertical,
  ListVideo,
  Loader2,
  RotateCcw,
  X,
} from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import {
  formatFileSize,
  getFFmpeg,
  getFileExtension,
  runFFmpeg,
  uid,
  type VideoOutputFormat,
  videoContainerCodecArgs,
} from "@/lib/ffmpeg/client";

const tool = getToolBySlug("video-merger");

export default function VideoMergerPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <VideoMergerTool />
    </ToolPageShell>
  );
}

interface VideoTrack {
  id: string;
  file: File;
  thumbnailUrl: string | null;
  duration: number;
  generating: boolean;
}

type MergeStatus = "idle" | "merging" | "done" | "error";

const ACCEPTED_VIDEO = ".mp4,.webm,.mkv,.mov,.avi";

const OUTPUT_FORMATS = [
  { label: "MP4", value: "mp4" },
  { label: "WebM", value: "webm" },
  { label: "MKV", value: "mkv" },
];

function formatDuration(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

async function generateThumbnail(
  file: File,
): Promise<{ url: string; duration: number }> {
  return new Promise((resolve) => {
    const video = document.createElement("video");
    const url = URL.createObjectURL(file);
    video.preload = "metadata";
    video.src = url;

    const cleanup = () => {
      video.src = "";
      URL.revokeObjectURL(url);
    };

    video.onloadedmetadata = () => {
      const duration = video.duration;
      video.currentTime = Math.min(0.5, duration / 2);

      video.onseeked = () => {
        const canvas = document.createElement("canvas");
        canvas.width = 160;
        canvas.height = 90;
        const ctx = canvas.getContext("2d")!;
        ctx.drawImage(video, 0, 0, 160, 90);
        canvas.toBlob(
          (blob) => {
            cleanup();
            if (blob) {
              resolve({ url: URL.createObjectURL(blob), duration });
            } else {
              resolve({ url: "", duration });
            }
          },
          "image/jpeg",
          0.75,
        );
      };
    };

    video.onerror = () => {
      cleanup();
      resolve({ url: "", duration: 0 });
    };
  });
}

function VideoMergerTool() {
  const [tracks, setTracks] = useState<VideoTrack[]>([]);
  const [outputFormat, setOutputFormat] = useState<VideoOutputFormat>("mp4");
  // On by default: concatenating clips whose size, frame rate or codecs
  // differ without normalising them yields a truncated or corrupt file.
  const [normalizeRes, setNormalizeRes] = useState(true);
  const [status, setStatus] = useState<MergeStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [progressLabel, setProgressLabel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [outputUrl, setOutputUrl] = useState<string | null>(null);
  const [outputSize, setOutputSize] = useState(0);

  const dragIdRef = useRef<string | null>(null);
  const dragOverIdRef = useRef<string | null>(null);
  const unmountUrlRef = useRef(outputUrl);
  const unmountTracksRef = useRef(tracks);

  useEffect(() => {
    unmountUrlRef.current = outputUrl;
    unmountTracksRef.current = tracks;
  }, [outputUrl, tracks]);

  useEffect(() => {
    return () => {
      if (unmountUrlRef.current) URL.revokeObjectURL(unmountUrlRef.current);
      unmountTracksRef.current.forEach((t) => {
        if (t.thumbnailUrl) URL.revokeObjectURL(t.thumbnailUrl);
      });
    };
  }, []);

  const addFiles = useCallback(async (files: File[]) => {
    const newTracks: VideoTrack[] = files.map((f) => ({
      id: uid("vtrack"),
      file: f,
      thumbnailUrl: null,
      duration: 0,
      generating: true,
    }));

    setTracks((prev) => [...prev, ...newTracks]);

    for (const track of newTracks) {
      const { url, duration } = await generateThumbnail(track.file);
      setTracks((prev) =>
        prev.map((t) =>
          t.id === track.id
            ? { ...t, thumbnailUrl: url, duration, generating: false }
            : t,
        ),
      );
    }
  }, []);

  const removeTrack = (id: string) => {
    setTracks((prev) => {
      const t = prev.find((t) => t.id === id);
      if (t?.thumbnailUrl) URL.revokeObjectURL(t.thumbnailUrl);
      return prev.filter((t) => t.id !== id);
    });
  };

  const handleDragStart = (id: string) => {
    dragIdRef.current = id;
  };
  const handleDragOver = (e: React.DragEvent, id: string) => {
    e.preventDefault();
    dragOverIdRef.current = id;
  };
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    const fromId = dragIdRef.current;
    const toId = dragOverIdRef.current;
    if (!fromId || !toId || fromId === toId) return;
    setTracks((prev) => {
      const list = [...prev];
      const fi = list.findIndex((t) => t.id === fromId);
      const ti = list.findIndex((t) => t.id === toId);
      const [item] = list.splice(fi, 1);
      list.splice(ti, 0, item);
      return list;
    });
    dragIdRef.current = null;
    dragOverIdRef.current = null;
  };

  const mergeAndExport = useCallback(async () => {
    if (tracks.length < 2) {
      setError("Add at least 2 video files.");
      return;
    }

    setStatus("merging");
    setProgress(0);
    setProgressLabel("Loading ffmpeg…");
    setError(null);
    if (outputUrl) URL.revokeObjectURL(outputUrl);
    setOutputUrl(null);

    const ffmpeg = await getFFmpeg();
    const jobId = uid("vmrg");

    try {
      const processedNames: string[] = [];

      if (normalizeRes) {
        // Get resolution of first track
        const firstExt = getFileExtension(tracks[0].file.name) || "mp4";
        const firstInput = `${jobId}_raw_0.${firstExt}`;
        await ffmpeg.writeFile(firstInput, await fetchFile(tracks[0].file));

        // Detect resolution of first file via log
        const logs: string[] = [];
        const onLog = ({ message }: { message: string }) => logs.push(message);
        ffmpeg.on("log", onLog);
        try {
          await ffmpeg.exec(["-i", firstInput]);
        } catch {}
        ffmpeg.off("log", onLog);

        const sizeMatch = logs
          .join("\n")
          .match(/Video:.*?\b(\d{2,5})x(\d{2,5})\b/);
        const targetW = sizeMatch ? parseInt(sizeMatch[1], 10) : 1280;
        const targetH = sizeMatch ? parseInt(sizeMatch[2], 10) : 720;

        for (let i = 0; i < tracks.length; i++) {
          setProgressLabel(`Normalizing track ${i + 1}/${tracks.length}…`);
          setProgress(Math.round((i / tracks.length) * 50));
          const ext = getFileExtension(tracks[i].file.name) || "mp4";
          const rawName = `${jobId}_raw_${i}.${ext}`;
          const normName = `${jobId}_norm_${i}.mp4`;

          if (i > 0) {
            await ffmpeg.writeFile(rawName, await fetchFile(tracks[i].file));
          }

          // Concat needs identical stream parameters, so frame rate, pixel
          // format and audio layout are pinned along with the frame size.
          await runFFmpeg(ffmpeg, [
            "-i",
            rawName,
            "-vf",
            `scale=${targetW}:${targetH}:force_original_aspect_ratio=decrease,pad=${targetW}:${targetH}:(ow-iw)/2:(oh-ih)/2,setsar=1`,
            "-r",
            "30",
            "-pix_fmt",
            "yuv420p",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-ar",
            "44100",
            "-ac",
            "2",
            normName,
          ]);
          processedNames.push(normName);
        }
      } else {
        for (let i = 0; i < tracks.length; i++) {
          setProgressLabel(`Loading track ${i + 1}/${tracks.length}…`);
          setProgress(Math.round((i / tracks.length) * 40));
          const ext = getFileExtension(tracks[i].file.name) || "mp4";
          const rawName = `${jobId}_raw_${i}.${ext}`;
          await ffmpeg.writeFile(rawName, await fetchFile(tracks[i].file));
          processedNames.push(rawName);
        }
      }

      setProgressLabel("Merging tracks…");
      setProgress(55);

      const concatContent = processedNames.map((n) => `file '${n}'`).join("\n");
      await ffmpeg.writeFile(
        "concat.txt",
        new TextEncoder().encode(concatContent),
      );

      const outputName = `merged_${jobId}.${outputFormat}`;

      let targetProgress = 55;
      const onProgress = ({ progress: r }: { progress: number }) => {
        targetProgress = Math.max(55, Math.min(95, 55 + Math.round(r * 40)));
      };
      let raf = 0;
      const animate = () => {
        setProgress((cur) =>
          Math.min(
            cur + Math.max(1, Math.round((targetProgress - cur) * 0.2)),
            targetProgress,
          ),
        );
        raf = requestAnimationFrame(animate);
      };
      ffmpeg.on("progress", onProgress);
      raf = requestAnimationFrame(animate);

      await runFFmpeg(ffmpeg, [
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        "concat.txt",
        ...videoContainerCodecArgs(outputFormat),
        outputName,
      ]);

      cancelAnimationFrame(raf);
      ffmpeg.off("progress", onProgress);

      const out = await ffmpeg.readFile(outputName);
      const blob = new Blob([(out as Uint8Array).slice()], {
        type: `video/${outputFormat}`,
      });
      setOutputUrl(URL.createObjectURL(blob));
      setOutputSize(blob.size);
      setProgress(100);
      setProgressLabel("Done!");
      setStatus("done");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Merge failed.");
      setStatus("error");
      setProgress(0);
    }
  }, [tracks, outputFormat, outputUrl, normalizeRes]);

  const download = () => {
    if (!outputUrl) return;
    const a = document.createElement("a");
    a.href = outputUrl;
    a.download = `merged_video.${outputFormat}`;
    a.click();
  };

  const totalDuration = tracks.reduce((s, t) => s + t.duration, 0);
  const hasBigFile = tracks.some((t) => t.file.size > 200 * 1024 * 1024);

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="video files"
        supportedFormats="mp4, webm, mkv, mov, avi"
        accept={ACCEPTED_VIDEO}
        multiple
        onFilesSelected={(files) => void addFiles(files)}
      />

      {hasBigFile && (
        <div className="flex items-start gap-3 rounded-2xl border border-amber-500/20 bg-amber-500/[0.07] p-3 text-amber-400">
          <AlertTriangle className="size-4 mt-0.5 shrink-0" />
          <p className="text-xs leading-relaxed">
            Large files detected (&gt;200MB). The bundled FFmpeg runs in memory
            and may be slow or run out of memory with very large inputs.
          </p>
        </div>
      )}

      {tracks.length > 0 && (
        <div className="space-y-2">
          {tracks.map((track, index) => (
            <div
              key={track.id}
              draggable
              onDragStart={() => handleDragStart(track.id)}
              onDragOver={(e) => handleDragOver(e, track.id)}
              onDrop={handleDrop}
              className="group flex items-center gap-3 rounded-2xl border border-white/10 bg-background/45 p-3 transition-colors hover:border-white/15 cursor-grab active:cursor-grabbing"
            >
              <GripVertical className="size-4 shrink-0 text-white/25 group-hover:text-white/50" />
              <span className="w-5 shrink-0 text-center text-xs text-muted-foreground">
                {index + 1}
              </span>

              {/* Thumbnail */}
              <div className="h-[68px] w-[120px] shrink-0 overflow-hidden rounded-lg border border-white/10 bg-black/40">
                {track.generating ? (
                  <div className="flex h-full w-full items-center justify-center">
                    <Loader2 className="size-4 animate-spin text-muted-foreground" />
                  </div>
                ) : track.thumbnailUrl ? (
                  // eslint-disable-next-line @next/next/no-img-element
                  <img
                    src={track.thumbnailUrl}
                    alt={track.file.name}
                    className="h-full w-full object-cover"
                  />
                ) : (
                  <div className="flex h-full w-full items-center justify-center text-xs text-muted-foreground">
                    no preview
                  </div>
                )}
              </div>

              {/* Info */}
              <div className="min-w-0 flex-1">
                <p
                  className="truncate text-sm font-medium"
                  title={track.file.name}
                >
                  {track.file.name}
                </p>
                <div className="mt-1 flex flex-wrap items-center gap-2">
                  <Badge
                    variant="outline"
                    className="border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px]"
                  >
                    {getFileExtension(track.file.name).toUpperCase()}
                  </Badge>
                  <span className="text-xs text-muted-foreground">
                    {formatFileSize(track.file.size)}
                  </span>
                  {track.duration > 0 && (
                    <span className="font-mono text-xs text-muted-foreground">
                      {formatDuration(track.duration)}
                    </span>
                  )}
                </div>
              </div>

              <Button
                variant="ghost"
                size="icon"
                className="size-8 shrink-0 text-muted-foreground"
                onClick={() => removeTrack(track.id)}
                disabled={status === "merging"}
              >
                <X className="size-4" />
              </Button>
            </div>
          ))}
        </div>
      )}

      {tracks.length === 0 && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <ListVideo className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Add 2 or more video files to get started.
          </p>
        </Card>
      )}

      {tracks.length >= 1 && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="flex flex-wrap items-center gap-4 px-5 py-3">
            <div className="text-xs text-muted-foreground">
              {totalDuration > 0 && (
                <>
                  Total:{" "}
                  <span className="font-mono">
                    {formatDuration(totalDuration)}
                  </span>
                  {" · "}
                </>
              )}
              {tracks.length} clip{tracks.length !== 1 ? "s" : ""}
            </div>

            {progressLabel && status === "merging" && (
              <span className="text-xs text-muted-foreground">
                {progressLabel}
              </span>
            )}

            <div className="ml-auto flex flex-wrap items-center gap-2">
              {/* Normalize toggle */}
              <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground">
                <div
                  role="checkbox"
                  aria-checked={normalizeRes}
                  onClick={() => setNormalizeRes((v) => !v)}
                  className={`relative h-4 w-7 rounded-full transition-colors ${normalizeRes ? "bg-yellow-400" : "bg-white/15"}`}
                >
                  <span
                    className={`absolute top-0.5 h-3 w-3 rounded-full bg-white transition-transform ${normalizeRes ? "left-3.5" : "left-0.5"}`}
                  />
                </div>
                Normalize resolution
              </label>

              <Select
                options={OUTPUT_FORMATS}
                value={outputFormat}
                onChange={(e) =>
                  setOutputFormat(e.target.value as VideoOutputFormat)
                }
                disabled={status === "merging"}
                className="h-8 w-24 border-white/10 bg-white/[0.04] text-xs shadow-none"
              />

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
                disabled={status === "merging" || tracks.length < 2}
                onClick={() => void mergeAndExport()}
              >
                {status === "merging" ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <RotateCcw className="size-3.5" />
                )}
                {status === "merging" ? "Merging…" : "Merge & Export"}
              </Button>
            </div>
          </div>

          {(status === "merging" || status === "done") && (
            <div className="space-y-1 px-5 pb-4 pt-3">
              <Progress value={progress} className="h-1.5 bg-white/10" />
              {status === "done" && outputSize > 0 && (
                <p className="text-xs text-muted-foreground">
                  Output: {formatFileSize(outputSize)} ·{" "}
                  {outputFormat.toUpperCase()}
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
