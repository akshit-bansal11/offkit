"use client";

import { fetchFile } from "@ffmpeg/util";
import JSZip from "jszip";
import { Download, Film, Loader2, X } from "lucide-react";
import Image from "next/image";
import React, { useCallback, useEffect, useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import {
  formatFileSize,
  getFFmpeg,
  getFileExtension,
  runFFmpeg,
  stripExtension,
  uid,
} from "@/lib/ffmpeg/client";

const tool = getToolBySlug("frames-extractor");

export default function FramesExtractorPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <FramesExtractorTool />
    </ToolPageShell>
  );
}

interface ExtractedFrame {
  name: string;
  blob: Blob;
  url: string;
}

interface ExtractionMeta {
  fps: number | null;
  frameCount: number;
}

const ACCEPTED_INPUT = ".mp4,.webm,.mov,.avi,.mkv,.gif";

function FramesExtractorTool() {
  const [file, setFile] = useState<File | null>(null);
  const [frames, setFrames] = useState<ExtractedFrame[]>([]);
  const [meta, setMeta] = useState<ExtractionMeta>({
    fps: null,
    frameCount: 0,
  });
  const [isExtracting, setIsExtracting] = useState(false);
  const [isDownloadingZip, setIsDownloadingZip] = useState(false);
  const [progress, setProgress] = useState(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // New features
  const [targetFps, setTargetFps] = useState<string>("auto");
  const [trimStart, setTrimStart] = useState<string>("");
  const [trimEnd, setTrimEnd] = useState<string>("");

  const clearFrames = useCallback(() => {
    frames.forEach((frame) => URL.revokeObjectURL(frame.url));
    setFrames([]);
    setMeta({ fps: null, frameCount: 0 });
  }, [frames]);

  useEffect(() => {
    return () => {
      frames.forEach((frame) => URL.revokeObjectURL(frame.url));
    };
  }, [frames]);

  const detectFps = useCallback(
    async (ffmpegInputName: string): Promise<number | null> => {
      const ffmpeg = await getFFmpeg();
      const logs: string[] = [];

      const onLog = ({ message }: { message: string }) => {
        logs.push(message);
      };

      ffmpeg.on("log", onLog);

      try {
        await ffmpeg.exec(["-i", ffmpegInputName]);
      } catch {
        // FFmpeg returns non-zero for probe-only command; logs are still useful.
      } finally {
        ffmpeg.off("log", onLog);
      }

      const combined = logs.join("\n");
      const match = combined.match(/(\d+(?:\.\d+)?)\s+fps/i);
      if (!match) {
        return null;
      }

      const parsed = Number.parseFloat(match[1]);
      return Number.isFinite(parsed) ? parsed : null;
    },
    [],
  );

  const extractFrames = useCallback(async () => {
    if (!file) {
      setErrorMessage("Upload a file first.");
      return;
    }

    setIsExtracting(true);
    setErrorMessage(null);
    setProgress(0);
    clearFrames();

    const ffmpeg = await getFFmpeg();
    const jobId = uid("frames");
    const sourceExt = getFileExtension(file.name) || "bin";
    const inputName = `${jobId}_input.${sourceExt}`;
    const framePattern = `${jobId}_frame_%04d.png`;

    let rafId = 0;
    let targetProgress = 0;

    const onProgress = ({ progress: ratio }: { progress: number }) => {
      targetProgress = Math.max(5, Math.min(96, Math.round(ratio * 96)));
    };

    const animate = () => {
      setProgress((current) => {
        if (current >= targetProgress) {
          return current;
        }
        const delta = Math.max(
          1,
          Math.round((targetProgress - current) * 0.25),
        );
        return Math.min(current + delta, targetProgress);
      });
      rafId = requestAnimationFrame(animate);
    };

    ffmpeg.on("progress", onProgress);
    rafId = requestAnimationFrame(animate);

    try {
      await ffmpeg.writeFile(inputName, await fetchFile(file));
      const detectedFps = await detectFps(inputName);

      const args = ["-i", inputName, "-vsync", "0"];

      if (trimStart) {
        args.splice(0, 0, "-ss", trimStart);
      }
      if (trimEnd) {
        // -to must be after -i if we want it to apply to the input file according to modern ffmpeg syntax
        // But for ffmpeg WASM, putting -ss and -to before -i enables fast seeking.
        if (trimStart) {
          args.splice(2, 0, "-to", trimEnd);
        } else {
          args.splice(0, 0, "-to", trimEnd);
        }
      }

      if (targetFps !== "auto") {
        // An fps filter, not -r: with -vsync 0 the output rate is ignored
        // and every source frame was written regardless of this setting.
        args.push("-vf", `fps=${targetFps}`);
      }

      args.push(framePattern);

      await runFFmpeg(ffmpeg, args);

      const extracted: ExtractedFrame[] = [];

      for (let index = 1; index < 100_000; index += 1) {
        const name = `${jobId}_frame_${String(index).padStart(4, "0")}.png`;
        try {
          const output = await ffmpeg.readFile(name);
          if (output.length === 0) {
            break;
          }

          const blob = new Blob([(output as Uint8Array).slice()], {
            type: "image/png",
          });
          const url = URL.createObjectURL(blob);
          extracted.push({
            name: `frame_${String(index).padStart(4, "0")}.png`,
            blob,
            url,
          });
        } catch {
          break;
        }
      }

      setFrames(extracted);

      const parsedFps =
        targetFps !== "auto" ? parseFloat(targetFps) : detectedFps;
      setMeta({ fps: parsedFps, frameCount: extracted.length });
      setProgress(100);
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : "Unable to extract frames from this media file.";
      setErrorMessage(message);
      setProgress(0);
    } finally {
      cancelAnimationFrame(rafId);
      ffmpeg.off("progress", onProgress);
      setIsExtracting(false);
    }
  }, [clearFrames, detectFps, file, targetFps, trimStart, trimEnd]);

  const downloadZip = useCallback(async () => {
    if (frames.length === 0) {
      return;
    }

    setIsDownloadingZip(true);
    setErrorMessage(null);

    try {
      const zip = new JSZip();
      frames.forEach((frame) => {
        zip.file(frame.name, frame.blob);
      });

      const blob = await zip.generateAsync({ type: "blob" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `${file ? stripExtension(file.name) : "frames"}-frames.zip`;
      link.click();
      URL.revokeObjectURL(url);
    } catch {
      setErrorMessage("Failed to generate ZIP archive.");
    } finally {
      setIsDownloadingZip(false);
    }
  }, [file, frames]);

  const infoText = useMemo(() => {
    const fpsText = meta.fps ? `${meta.fps.toFixed(2)} fps` : "FPS unavailable";
    return `${meta.frameCount} frames • ${fpsText}`;
  }, [meta]);

  return (
    <div className="space-y-6">
      <Card className="tool-card-inline">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-lg">
            <Film className="size-5 text-primary" />
            GIF & video frame extractor
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-5">
          <FileDropZoneCard
            fileTypeLabel="a GIF or video file"
            supportedFormats="mp4, webm, mov, avi, mkv, and gif"
            accept={ACCEPTED_INPUT}
            onFilesSelected={(incoming) => {
              const selected = incoming[0] ?? null;
              if (!selected) {
                return;
              }

              const ext = getFileExtension(selected.name);
              const allowed = ["mp4", "webm", "mov", "avi", "mkv", "gif"];
              if (!allowed.includes(ext)) {
                setErrorMessage(
                  "Unsupported file type. Please upload mp4, webm, mov, avi, mkv, or gif.",
                );
                return;
              }

              clearFrames();
              setErrorMessage(null);
              setProgress(0);
              setFile(selected);
            }}
          />

          {file ? (
            <div className="rounded-xl border border-white/10 bg-background/40 p-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p className="font-medium">{file.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {formatFileSize(file.size)}
                  </p>
                </div>
                <Badge
                  variant="outline"
                  className="border-white/15 bg-background/70"
                >
                  {getFileExtension(file.name).toUpperCase()}
                </Badge>
              </div>

              <div className="mt-4 grid gap-4 grid-cols-1 sm:grid-cols-3">
                <div className="space-y-1.5">
                  <label className="text-xs text-muted-foreground">
                    Extract Frequency
                  </label>
                  <select
                    className="flex h-9 w-full items-center justify-between whitespace-nowrap rounded-md border border-white/10 bg-transparent px-3 py-2 text-sm shadow-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1"
                    value={targetFps}
                    onChange={(e) => setTargetFps(e.target.value)}
                    disabled={isExtracting}
                  >
                    <option
                      value="auto"
                      className="bg-background text-foreground"
                    >
                      Extract every frame (Auto)
                    </option>
                    <option
                      value="30"
                      className="bg-background text-foreground"
                    >
                      30 FPS
                    </option>
                    <option
                      value="10"
                      className="bg-background text-foreground"
                    >
                      10 FPS
                    </option>
                    <option value="5" className="bg-background text-foreground">
                      5 FPS
                    </option>
                    <option value="1" className="bg-background text-foreground">
                      1 FPS (1 frame/sec)
                    </option>
                    <option
                      value="0.5"
                      className="bg-background text-foreground"
                    >
                      0.5 FPS (1 frame/2s)
                    </option>
                    <option
                      value="0.1"
                      className="bg-background text-foreground"
                    >
                      0.1 FPS (1 frame/10s)
                    </option>
                  </select>
                </div>

                <div className="space-y-1.5">
                  <label className="text-xs text-muted-foreground">
                    Start Time (sec){" "}
                    <span className="opacity-50 text-[10px]">Optional</span>
                  </label>
                  <input
                    type="number"
                    min="0"
                    step="0.1"
                    placeholder="e.g. 1.5"
                    value={trimStart}
                    onChange={(e) => setTrimStart(e.target.value)}
                    disabled={isExtracting}
                    className="flex h-9 w-full rounded-md border border-white/10 bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                  />
                </div>

                <div className="space-y-1.5">
                  <label className="text-xs text-muted-foreground">
                    End Time (sec){" "}
                    <span className="opacity-50 text-[10px]">Optional</span>
                  </label>
                  <input
                    type="number"
                    min="0"
                    step="0.1"
                    placeholder="e.g. 5"
                    value={trimEnd}
                    onChange={(e) => setTrimEnd(e.target.value)}
                    disabled={isExtracting}
                    className="flex h-9 w-full rounded-md border border-white/10 bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                  />
                </div>
              </div>

              <div className="mt-5 flex flex-wrap items-center gap-2">
                <Button
                  onClick={() => void extractFrames()}
                  disabled={isExtracting}
                >
                  {isExtracting ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Film className="size-4" />
                  )}
                  Extract frames
                </Button>
                <Button
                  variant="outline"
                  disabled={frames.length === 0 || isDownloadingZip}
                  onClick={() => void downloadZip()}
                >
                  {isDownloadingZip ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Download className="size-4" />
                  )}
                  Download ZIP
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setFile(null);
                    setProgress(0);
                    setTrimStart("");
                    setTrimEnd("");
                    setTargetFps("auto");
                    clearFrames();
                  }}
                  disabled={isExtracting}
                >
                  <X className="size-4" />
                  Reset
                </Button>
              </div>

              <div className="mt-4 space-y-2">
                <Progress value={progress} />
                <p className="text-xs text-muted-foreground">{infoText}</p>
              </div>
            </div>
          ) : null}

          {errorMessage ? (
            <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {errorMessage}
            </div>
          ) : null}
        </CardContent>
      </Card>

      {frames.length > 0 ? (
        <Card className="tool-card-inline">
          <CardHeader className="border-b border-white/10">
            <CardTitle className="text-lg">
              Extracted frame thumbnails
            </CardTitle>
          </CardHeader>
          <CardContent className="p-5">
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4 xl:grid-cols-6">
              {frames.map((frame, index) => (
                <div
                  key={frame.name}
                  className="group relative overflow-hidden rounded-lg border border-white/10 bg-background/30"
                >
                  <Image
                    src={frame.url}
                    alt={frame.name}
                    width={360}
                    height={220}
                    className="h-28 w-full object-cover"
                    unoptimized
                  />
                  <div className="absolute inset-x-0 bottom-0 bg-black/65 px-2 py-1 text-xs text-white">
                    #{index + 1}
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
