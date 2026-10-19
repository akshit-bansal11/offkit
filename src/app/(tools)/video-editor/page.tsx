"use client";

import { fetchFile } from "@ffmpeg/util";
import {
  AlertTriangle,
  Download,
  Film,
  FlipHorizontal,
  FlipVertical,
  Loader2,
  Minus,
  Plus,
  RotateCcw,
  RotateCw,
  X,
  Zap,
} from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { Input } from "@/components/ui/form/Input";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { Card } from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";
import {
  formatFileSize,
  getFFmpeg,
  getFileExtension,
  runFFmpeg,
  uid,
  videoContainerCodecArgs,
} from "@/lib/ffmpeg/client";
import {
  buildAudioFilterChain,
  buildVideoFilterChain,
} from "@/lib/tools/video-editor/filters";
import type {
  RotateAngle,
  VideoEditorState,
  VideoOutputFormat,
  VideoTabId,
} from "@/lib/tools/video-editor/types";

const tool = getToolBySlug("video-editor");

export default function VideoEditorPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <VideoEditorTool />
    </ToolPageShell>
  );
}

const ACCEPTED_VIDEO = ".mp4,.webm,.mkv,.mov,.avi";

const OUTPUT_FORMATS = [
  { label: "MP4", value: "mp4" },
  { label: "WebM", value: "webm" },
  { label: "MKV", value: "mkv" },
];

const RESOLUTION_OPTIONS = [
  { label: "Original", value: "original" },
  { label: "4K (2160p)", value: "2160" },
  { label: "1080p", value: "1080" },
  { label: "720p", value: "720" },
  { label: "480p", value: "480" },
];

const SPEED_PRESETS = [0.25, 0.5, 1, 1.5, 2, 4];

function formatTime(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = (sec % 60).toFixed(1).padStart(4, "0");
  return `${m}:${s}`;
}

function initState(): VideoEditorState {
  return {
    file: null,
    videoUrl: null,
    duration: 0,
    status: "idle",
    error: null,
    progress: 0,
    activeTab: "trim",
    trimStart: 0,
    trimEnd: 0,
    cropX: 0,
    cropY: 0,
    cropW: 0,
    cropH: 0,
    cropEnabled: false,
    rotateAngle: 0,
    resolution: "original",
    customW: 0,
    customH: 0,
    aspectLock: true,
    speed: 1,
    volume: 100,
    flipH: false,
    flipV: false,
    outputFormat: "mp4",
    outputUrl: null,
    outputSize: 0,
  };
}

const TAB_LABELS: { id: VideoTabId; label: string }[] = [
  { id: "trim", label: "Trim" },
  { id: "crop", label: "Crop" },
  { id: "rotate", label: "Rotate" },
  { id: "resize", label: "Resize" },
  { id: "speed", label: "Speed" },
  { id: "volume", label: "Volume" },
  { id: "flip", label: "Flip" },
];

function VideoEditorTool() {
  const [state, setState] = useState<VideoEditorState>(initState());
  const videoRef = useRef<HTMLVideoElement>(null);
  const videoUrlRef = useRef<string | null>(null);
  const outputUrlRef = useRef<string | null>(null);

  const patch = useCallback((p: Partial<VideoEditorState>) => {
    setState((prev) => {
      const next = { ...prev, ...p };
      if (p.videoUrl !== undefined) videoUrlRef.current = p.videoUrl;
      if (p.outputUrl !== undefined) outputUrlRef.current = p.outputUrl;
      return next;
    });
  }, []);

  useEffect(() => {
    return () => {
      if (videoUrlRef.current) URL.revokeObjectURL(videoUrlRef.current);
      if (outputUrlRef.current) URL.revokeObjectURL(outputUrlRef.current);
    };
  }, []);

  const handleFile = useCallback(
    async (file: File) => {
      if (state.videoUrl) URL.revokeObjectURL(state.videoUrl);
      if (state.outputUrl) URL.revokeObjectURL(state.outputUrl);

      const videoUrl = URL.createObjectURL(file);
      patch({
        file,
        videoUrl,
        status: "loading",
        error: null,
        outputUrl: null,
        outputSize: 0,
      });

      // Get duration from video element
      const video = document.createElement("video");
      video.preload = "metadata";
      video.src = videoUrl;
      video.onloadedmetadata = () => {
        const duration = video.duration;
        patch({ duration, trimEnd: duration, status: "ready" });
      };
      video.onerror = () => {
        patch({ status: "error", error: "Failed to load video metadata." });
      };
    },
    [state.videoUrl, state.outputUrl, patch],
  );

  const exportVideo = useCallback(async () => {
    const { file, trimStart, trimEnd, duration } = state;
    if (!file) return;

    patch({ status: "exporting", progress: 0, error: null });

    const ffmpeg = await getFFmpeg();
    const jobId = uid("vedit");
    const ext = getFileExtension(file.name) || "mp4";
    const inputName = `${jobId}_input.${ext}`;
    const outputName = `${jobId}_output.${state.outputFormat}`;

    let targetProgress = 0;
    const onProgress = ({ progress: r }: { progress: number }) => {
      targetProgress = Math.max(5, Math.min(95, Math.round(r * 95)));
    };
    let raf = 0;
    const animate = () => {
      patch({
        progress: Math.min(
          state.progress +
            Math.max(1, Math.round((targetProgress - state.progress) * 0.2)),
          targetProgress,
        ),
      });
      raf = requestAnimationFrame(animate);
    };

    try {
      await ffmpeg.writeFile(inputName, await fetchFile(file));

      ffmpeg.on("progress", onProgress);
      raf = requestAnimationFrame(animate);

      const args: string[] = [];

      // Seek args (before -i for fast seeking)
      if (trimStart > 0.01) {
        args.push("-ss", trimStart.toFixed(3));
      }

      args.push("-i", inputName);

      if (trimEnd < duration - 0.05) {
        args.push("-to", (trimEnd - trimStart).toFixed(3));
      }

      const vf = buildVideoFilterChain(state);
      const af = buildAudioFilterChain(state);
      if (vf) args.push("-vf", vf);
      if (af) args.push("-af", af);

      args.push(...videoContainerCodecArgs(state.outputFormat), outputName);

      await runFFmpeg(ffmpeg, args);

      cancelAnimationFrame(raf);
      ffmpeg.off("progress", onProgress);

      const out = await ffmpeg.readFile(outputName);
      const blob = new Blob([(out as Uint8Array).slice()], {
        type: `video/${state.outputFormat}`,
      });

      if (state.outputUrl) URL.revokeObjectURL(state.outputUrl);
      patch({
        status: "done",
        progress: 100,
        outputUrl: URL.createObjectURL(blob),
        outputSize: blob.size,
      });
    } catch (e) {
      cancelAnimationFrame(raf);
      ffmpeg.off("progress", onProgress);
      patch({
        status: "error",
        error: e instanceof Error ? e.message : "Export failed.",
        progress: 0,
      });
    }
  }, [state, patch]);

  const download = () => {
    if (!state.outputUrl) return;
    const a = document.createElement("a");
    a.href = state.outputUrl;
    a.download = `edited_video.${state.outputFormat}`;
    a.click();
  };

  const isReady =
    state.status === "ready" ||
    state.status === "done" ||
    state.status === "exporting";
  const isBigFile = state.file && state.file.size > 50 * 1024 * 1024;

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="a video file"
        supportedFormats="mp4, webm, mkv, mov, avi"
        accept={ACCEPTED_VIDEO}
        multiple={false}
        onFilesSelected={(files) => {
          const f = files[0];
          if (f) void handleFile(f);
        }}
      />

      {state.status === "loading" && (
        <Card className="flex items-center gap-3 border-white/10 bg-background/30 p-6">
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
          <p className="text-sm text-muted-foreground">Loading video…</p>
        </Card>
      )}

      {isBigFile && isReady && (
        <div className="flex items-start gap-3 rounded-2xl border border-warning/25 bg-warning/10 p-3 text-warning">
          <AlertTriangle className="mt-0.5 size-4 shrink-0" />
          <p className="text-xs leading-relaxed">
            File is larger than 50 MB. The bundled FFmpeg runs in memory and
            will be slow for large files.
          </p>
        </div>
      )}

      {isReady && state.file && (
        <>
          {/* File info row */}
          <div className="flex flex-wrap items-center gap-3 rounded-2xl border border-white/10 bg-background/45 px-4 py-3">
            <Film className="size-4 text-muted-foreground" />
            <p className="min-w-0 flex-1 truncate text-sm font-medium">
              {state.file.name}
            </p>
            <Badge
              variant="outline"
              className="border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px]"
            >
              {getFileExtension(state.file.name).toUpperCase()}
            </Badge>
            <span className="text-xs text-muted-foreground">
              {formatFileSize(state.file.size)}
            </span>
            {state.duration > 0 && (
              <span className="font-mono text-xs text-muted-foreground">
                {formatTime(state.duration)}
              </span>
            )}
            <Button
              variant="ghost"
              size="icon"
              aria-label="Remove video"
              className="size-7 text-muted-foreground"
              onClick={() => setState(initState())}
            >
              <X className="size-3.5" />
            </Button>
          </div>

          {/* Video preview */}
          <div className="overflow-hidden rounded-2xl border border-white/10 bg-black">
            <video
              ref={videoRef}
              src={state.videoUrl ?? undefined}
              controls
              className="w-full max-h-[400px]"
            />
          </div>

          {/* Timeline (simple trim bar) */}
          <Card className="border-white/10 bg-white/[0.015] p-4">
            <p className="mb-2 text-xs text-muted-foreground">
              Timeline (Trim)
            </p>
            <div className="relative h-8 w-full overflow-hidden rounded-lg bg-white/[0.06]">
              {/* Selected region */}
              <div
                className="absolute inset-y-0 bg-warning/20 border-x border-warning/50"
                style={{
                  left: `${(state.trimStart / Math.max(state.duration, 1)) * 100}%`,
                  width: `${((state.trimEnd - state.trimStart) / Math.max(state.duration, 1)) * 100}%`,
                }}
              />
              {/* Start handle */}
              <input
                type="range"
                aria-label="Trim start"
                min={0}
                max={state.duration}
                step={0.1}
                value={state.trimStart}
                onChange={(e) => {
                  const val = Math.min(
                    Number(e.target.value),
                    state.trimEnd - 0.1,
                  );
                  patch({ trimStart: parseFloat(val.toFixed(1)) });
                }}
                className="pointer-events-none absolute inset-0 h-full w-full cursor-pointer appearance-none bg-transparent focus-visible:outline-none [&::-webkit-slider-thumb]:pointer-events-auto [&::-webkit-slider-thumb]:h-8 [&::-webkit-slider-thumb]:w-2 [&::-webkit-slider-thumb]:cursor-grab [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-sm [&::-webkit-slider-thumb]:bg-warning [&::-moz-range-thumb]:pointer-events-auto [&::-moz-range-thumb]:h-8 [&::-moz-range-thumb]:w-2 [&::-moz-range-thumb]:cursor-grab [&::-moz-range-thumb]:rounded-sm [&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:bg-warning"
              />
              {/* End handle */}
              <input
                type="range"
                aria-label="Trim end"
                min={0}
                max={state.duration}
                step={0.1}
                value={state.trimEnd}
                onChange={(e) => {
                  const val = Math.max(
                    Number(e.target.value),
                    state.trimStart + 0.1,
                  );
                  patch({ trimEnd: parseFloat(val.toFixed(1)) });
                }}
                className="pointer-events-none absolute inset-0 h-full w-full cursor-pointer appearance-none bg-transparent focus-visible:outline-none [&::-webkit-slider-thumb]:pointer-events-auto [&::-webkit-slider-thumb]:h-8 [&::-webkit-slider-thumb]:w-2 [&::-webkit-slider-thumb]:cursor-grab [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-sm [&::-webkit-slider-thumb]:bg-warning [&::-moz-range-thumb]:pointer-events-auto [&::-moz-range-thumb]:h-8 [&::-moz-range-thumb]:w-2 [&::-moz-range-thumb]:cursor-grab [&::-moz-range-thumb]:rounded-sm [&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:bg-warning"
              />
            </div>
            <div className="mt-2 flex justify-between text-xs font-mono text-muted-foreground">
              <span>{formatTime(state.trimStart)}</span>
              <span>{formatTime(state.trimEnd)}</span>
            </div>
          </Card>

          {/* Tab controls */}
          <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
            <div className="flex w-full items-center justify-center p-3">
              <SegmentedControl
                variant="dark"
                value={state.activeTab}
                onValueChange={(v) => patch({ activeTab: v as VideoTabId })}
                className="max-w-full flex-wrap rounded-xl border bg-card/60 p-1"
                optionClassName="rounded-lg px-3 py-1.5 text-xs whitespace-nowrap"
                options={TAB_LABELS.map((t) => ({
                  label: t.label,
                  value: t.id,
                }))}
              />
            </div>

            {/* Tab contents */}
            <div className="p-5">
              {/* ── Trim ── */}
              {state.activeTab === "trim" && (
                <div className="space-y-4">
                  <p className="text-sm text-muted-foreground">
                    Drag the timeline handles above, or use the nudge buttons.
                  </p>
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                      <p className="text-xs text-muted-foreground">Start</p>
                      <div className="flex items-center gap-1.5">
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label="Start earlier"
                          className="size-8 border border-white/10"
                          onClick={() =>
                            patch({
                              trimStart: Math.max(
                                0,
                                parseFloat((state.trimStart - 0.1).toFixed(1)),
                              ),
                            })
                          }
                        >
                          <Minus className="size-3.5" />
                        </Button>
                        <span className="flex-1 text-center font-mono text-sm tabular-nums">
                          {formatTime(state.trimStart)}
                        </span>
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label="Start later"
                          className="size-8 border border-white/10"
                          onClick={() =>
                            patch({
                              trimStart: Math.min(
                                state.trimEnd - 0.1,
                                parseFloat((state.trimStart + 0.1).toFixed(1)),
                              ),
                            })
                          }
                        >
                          <Plus className="size-3.5" />
                        </Button>
                      </div>
                    </div>
                    <div className="space-y-2">
                      <p className="text-xs text-muted-foreground">End</p>
                      <div className="flex items-center gap-1.5">
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label="End earlier"
                          className="size-8 border border-white/10"
                          onClick={() =>
                            patch({
                              trimEnd: Math.max(
                                state.trimStart + 0.1,
                                parseFloat((state.trimEnd - 0.1).toFixed(1)),
                              ),
                            })
                          }
                        >
                          <Minus className="size-3.5" />
                        </Button>
                        <span className="flex-1 text-center font-mono text-sm tabular-nums">
                          {formatTime(state.trimEnd)}
                        </span>
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label="End later"
                          className="size-8 border border-white/10"
                          onClick={() =>
                            patch({
                              trimEnd: Math.min(
                                state.duration,
                                parseFloat((state.trimEnd + 0.1).toFixed(1)),
                              ),
                            })
                          }
                        >
                          <Plus className="size-3.5" />
                        </Button>
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Badge
                      variant="outline"
                      className="border-white/10 bg-white/[0.04] font-mono text-xs"
                    >
                      Clip: {formatTime(state.trimEnd - state.trimStart)}
                    </Badge>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="ml-auto h-7 border border-white/10 text-xs text-muted-foreground"
                      onClick={() =>
                        patch({ trimStart: 0, trimEnd: state.duration })
                      }
                    >
                      Reset
                    </Button>
                  </div>

                  {/* Trim range inputs */}
                  <div className="space-y-2">
                    <p className="text-xs text-muted-foreground">
                      Fine-tune trim range
                    </p>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground w-8 shrink-0">
                        Start
                      </span>
                      <input
                        type="range"
                        min={0}
                        max={state.duration}
                        step={0.1}
                        value={state.trimStart}
                        onChange={(e) => {
                          const v = Math.min(
                            Number(e.target.value),
                            state.trimEnd - 0.1,
                          );
                          patch({ trimStart: parseFloat(v.toFixed(1)) });
                        }}
                        className="flex-1"
                      />
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground w-8 shrink-0">
                        End
                      </span>
                      <input
                        type="range"
                        min={0}
                        max={state.duration}
                        step={0.1}
                        value={state.trimEnd}
                        onChange={(e) => {
                          const v = Math.max(
                            Number(e.target.value),
                            state.trimStart + 0.1,
                          );
                          patch({ trimEnd: parseFloat(v.toFixed(1)) });
                        }}
                        className="flex-1"
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* ── Crop ── */}
              {state.activeTab === "crop" && (
                <div className="space-y-4">
                  <div className="flex items-center gap-3">
                    <p className="text-sm text-muted-foreground flex-1">
                      Set crop region in pixels (relative to original video
                      dimensions).
                    </p>
                    <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground">
                      <button
                        type="button"
                        role="switch"
                        aria-checked={state.cropEnabled}
                        onClick={() =>
                          patch({ cropEnabled: !state.cropEnabled })
                        }
                        className={`relative h-4 w-7 cursor-pointer rounded-full transition-colors motion-reduce:transition-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${state.cropEnabled ? "bg-warning" : "bg-white/15 hover:bg-white/25"}`}
                      >
                        <span
                          className={`absolute top-0.5 h-3 w-3 rounded-full bg-white transition-transform motion-reduce:transition-none ${state.cropEnabled ? "left-3.5" : "left-0.5"}`}
                        />
                      </button>
                      Enabled
                    </label>
                  </div>
                  <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                    {(["cropX", "cropY", "cropW", "cropH"] as const).map(
                      (key) => (
                        <div key={key} className="space-y-1.5">
                          <p className="text-xs text-muted-foreground uppercase tracking-wider">
                            {key.replace("crop", "")}
                          </p>
                          <Input
                            type="number"
                            min={0}
                            value={state[key]}
                            onChange={(e) =>
                              patch({
                                [key]: parseInt(e.target.value, 10) || 0,
                              } as Partial<VideoEditorState>)
                            }
                            disabled={!state.cropEnabled}
                            className="h-9 border-white/10 bg-white/[0.04] text-sm font-mono"
                          />
                        </div>
                      ),
                    )}
                  </div>
                  {state.cropEnabled && state.cropW > 0 && state.cropH > 0 && (
                    <p className="text-xs text-muted-foreground font-mono">
                      Region: {state.cropW}×{state.cropH} at ({state.cropX},{" "}
                      {state.cropY})
                    </p>
                  )}
                </div>
              )}

              {/* ── Rotate ── */}
              {state.activeTab === "rotate" && (
                <div className="space-y-4">
                  <p className="text-sm text-muted-foreground">
                    Rotate the video by a fixed angle.
                  </p>
                  <div className="flex flex-wrap gap-2">
                    {([0, 90, 180, 270] as RotateAngle[]).map((angle) => (
                      <Button
                        key={angle}
                        variant={
                          state.rotateAngle === angle ? "default" : "outline"
                        }
                        size="sm"
                        className={`h-10 gap-2 border-white/10 ${state.rotateAngle === angle ? "" : "bg-white/[0.04]"}`}
                        onClick={() => patch({ rotateAngle: angle })}
                      >
                        {angle === 0 && <RotateCcw className="size-4" />}
                        {angle === 90 && <RotateCw className="size-4" />}
                        {angle === 180 && <RotateCcw className="size-4" />}
                        {angle === 270 && <RotateCcw className="size-4" />}
                        {angle}°
                      </Button>
                    ))}
                  </div>
                </div>
              )}

              {/* ── Resize ── */}
              {state.activeTab === "resize" && (
                <div className="space-y-4">
                  <div className="space-y-2">
                    <p className="text-sm font-medium">Resolution Preset</p>
                    <Select
                      options={RESOLUTION_OPTIONS}
                      value={state.resolution}
                      onChange={(e) =>
                        patch({
                          resolution: e.target
                            .value as VideoEditorState["resolution"],
                        })
                      }
                      className="h-9 border-white/10 bg-white/[0.04] text-sm"
                    />
                  </div>

                  {state.resolution === "original" && (
                    <div className="grid grid-cols-2 gap-3">
                      <div className="space-y-1.5">
                        <p className="text-xs text-muted-foreground">
                          Custom Width (px)
                        </p>
                        <Input
                          type="number"
                          min={0}
                          value={state.customW || ""}
                          onChange={(e) =>
                            patch({
                              customW: parseInt(e.target.value, 10) || 0,
                            })
                          }
                          placeholder="e.g. 1920"
                          className="h-9 border-white/10 bg-white/[0.04] text-sm"
                        />
                      </div>
                      <div className="space-y-1.5">
                        <p className="text-xs text-muted-foreground">
                          Custom Height (px)
                        </p>
                        <Input
                          type="number"
                          min={0}
                          value={state.customH || ""}
                          onChange={(e) =>
                            patch({
                              customH: parseInt(e.target.value, 10) || 0,
                            })
                          }
                          placeholder="e.g. 1080"
                          className="h-9 border-white/10 bg-white/[0.04] text-sm"
                        />
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* ── Speed ── */}
              {state.activeTab === "speed" && (
                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <p className="text-sm font-medium">Playback Speed</p>
                    <Badge
                      variant="outline"
                      className="border-white/10 bg-white/[0.04] font-mono text-xs"
                    >
                      {state.speed}×
                    </Badge>
                  </div>
                  <Slider
                    min={0.25}
                    max={4}
                    step={0.25}
                    value={state.speed}
                    onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                      patch({ speed: Number(e.target.value) })
                    }
                  />
                  <div className="flex flex-wrap gap-1.5">
                    {SPEED_PRESETS.map((s) => (
                      <Button
                        key={s}
                        variant={state.speed === s ? "default" : "outline"}
                        size="sm"
                        className={`h-7 w-12 border-white/10 text-xs ${state.speed === s ? "" : "bg-white/[0.04]"}`}
                        onClick={() => patch({ speed: s })}
                      >
                        {s}×
                      </Button>
                    ))}
                  </div>
                </div>
              )}

              {/* ── Volume ── */}
              {state.activeTab === "volume" && (
                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <p className="text-sm font-medium">Volume</p>
                    <Badge
                      variant="outline"
                      className="border-white/10 bg-white/[0.04] font-mono text-xs"
                    >
                      {state.volume}%
                    </Badge>
                  </div>
                  <Slider
                    min={0}
                    max={200}
                    step={5}
                    value={state.volume}
                    onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                      patch({ volume: Number(e.target.value) })
                    }
                  />
                  <div className="flex flex-wrap gap-1.5">
                    {[0, 50, 100, 150, 200].map((v) => (
                      <Button
                        key={v}
                        variant={state.volume === v ? "default" : "outline"}
                        size="sm"
                        className={`h-7 border-white/10 text-xs ${state.volume === v ? "" : "bg-white/[0.04]"}`}
                        onClick={() => patch({ volume: v })}
                      >
                        {v}%
                      </Button>
                    ))}
                  </div>
                </div>
              )}

              {/* ── Flip ── */}
              {state.activeTab === "flip" && (
                <div className="space-y-4">
                  <p className="text-sm text-muted-foreground">
                    Mirror the video horizontally or vertically.
                  </p>
                  <div className="flex gap-3">
                    <Button
                      variant={state.flipH ? "default" : "outline"}
                      size="sm"
                      className={`h-10 gap-2 border-white/10 ${!state.flipH ? "bg-white/[0.04]" : ""}`}
                      onClick={() => patch({ flipH: !state.flipH })}
                    >
                      <FlipHorizontal className="size-4" />
                      Horizontal
                    </Button>
                    <Button
                      variant={state.flipV ? "default" : "outline"}
                      size="sm"
                      className={`h-10 gap-2 border-white/10 ${!state.flipV ? "bg-white/[0.04]" : ""}`}
                      onClick={() => patch({ flipV: !state.flipV })}
                    >
                      <FlipVertical className="size-4" />
                      Vertical
                    </Button>
                  </div>
                </div>
              )}
            </div>
          </Card>

          {/* Export bar */}
          <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
            <div className="flex flex-wrap items-center gap-3 px-5 py-3">
              <Zap className="size-4 text-warning" />
              <p className="text-sm font-medium">Export</p>

              <div className="ml-auto flex flex-wrap items-center gap-2">
                <Select
                  options={OUTPUT_FORMATS}
                  value={state.outputFormat}
                  onChange={(e) =>
                    patch({ outputFormat: e.target.value as VideoOutputFormat })
                  }
                  disabled={state.status === "exporting"}
                  className="h-8 w-24 border-white/10 bg-white/[0.04] text-xs shadow-none"
                />

                {state.outputUrl && (
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
                  disabled={state.status === "exporting"}
                  onClick={() => void exportVideo()}
                >
                  {state.status === "exporting" ? (
                    <Loader2 className="size-3.5 animate-spin" />
                  ) : (
                    <Film className="size-3.5" />
                  )}
                  {state.status === "exporting" ? "Exporting…" : "Export Video"}
                </Button>
              </div>
            </div>

            {(state.status === "exporting" || state.status === "done") && (
              <div className="space-y-1 px-5 pb-4 pt-3">
                <Progress
                  value={state.progress}
                  className="h-1.5 bg-white/10"
                />
                {state.status === "done" && state.outputSize > 0 && (
                  <p className="text-xs text-muted-foreground">
                    Output: {formatFileSize(state.outputSize)} ·{" "}
                    {state.outputFormat.toUpperCase()}
                  </p>
                )}
              </div>
            )}
          </Card>
        </>
      )}

      {state.status === "idle" && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <Film className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Upload a video to start editing.
          </p>
        </Card>
      )}

      {state.error && (
        <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {state.error}
        </div>
      )}
    </div>
  );
}
