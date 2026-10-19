"use client";

import { fetchFile } from "@ffmpeg/util";
import {
  ChevronDown,
  ChevronUp,
  Download,
  GripVertical,
  ListMusic,
  Loader2,
  Minus,
  Plus,
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
import { WaveformCanvas } from "@/components/ui/interaction/WaveformCanvas";
import { Card } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import { decodeAudioFile, formatDuration } from "@/lib/audio/utils";
import {
  audioCodecArgs,
  formatFileSize,
  getFFmpeg,
  getFileExtension,
  runFFmpeg,
  uid,
} from "@/lib/ffmpeg/client";

// ─── Page shell ──────────────────────────────────────────────────────────────

const tool = getToolBySlug("audio-joiner");

export default function AudioJoinerPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <AudioJoinerTool />
    </ToolPageShell>
  );
}

// ─── Constants ───────────────────────────────────────────────────────────────

const ACCEPTED_AUDIO = ".mp3,.wav,.flac,.aac,.m4a,.opus,.ogg";

const OUTPUT_FORMATS = [
  { label: "MP3", value: "mp3" },
  { label: "WAV", value: "wav" },
  { label: "FLAC", value: "flac" },
  { label: "AAC", value: "aac" },
  { label: "OPUS", value: "opus" },
];

const CODEC_FOR_FORMAT: Record<string, string> = {
  mp3: "libmp3lame",
  wav: "pcm_s16le",
  flac: "flac",
  aac: "aac",
  opus: "opus",
};

type JoinStatus = "idle" | "joining" | "done" | "error";

// ─── Track item ───────────────────────────────────────────────────────────────

interface TrackItem {
  id: string;
  file: File;
  audioBuffer: AudioBuffer | null;
  duration: number;
  decoding: boolean;
  /** Per-track trim start in seconds (default: 0) */
  trimStart: number;
  /** Per-track trim end in seconds (default: duration) */
  trimEnd: number | null;
  /** Whether the trim panel is expanded */
  trimExpanded: boolean;
}

// ─── Tool ────────────────────────────────────────────────────────────────────

function AudioJoinerTool() {
  const [tracks, setTracks] = useState<TrackItem[]>([]);
  const [outputFormat, setOutputFormat] = useState("mp3");
  const [status, setStatus] = useState<JoinStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [progressLabel, setProgressLabel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [outputUrl, setOutputUrl] = useState<string | null>(null);
  const [outputSize, setOutputSize] = useState(0);

  const dragIdRef = useRef<string | null>(null);
  const dragOverIdRef = useRef<string | null>(null);

  useEffect(() => {
    return () => {
      if (outputUrl) URL.revokeObjectURL(outputUrl);
    };
  }, [outputUrl]);

  // ── Add files ──────────────────────────────────────────────────────────────
  const addFiles = useCallback(async (files: File[]) => {
    const newTracks: TrackItem[] = files.map((f) => ({
      id: uid("track"),
      file: f,
      audioBuffer: null,
      duration: 0,
      decoding: true,
      trimStart: 0,
      trimEnd: null,
      trimExpanded: false,
    }));

    setTracks((prev) => [...prev, ...newTracks]);

    for (const track of newTracks) {
      try {
        const buf = await decodeAudioFile(track.file);
        setTracks((prev) =>
          prev.map((t) =>
            t.id === track.id
              ? {
                  ...t,
                  audioBuffer: buf,
                  duration: buf.duration,
                  trimEnd: buf.duration,
                  decoding: false,
                }
              : t,
          ),
        );
      } catch {
        setTracks((prev) =>
          prev.map((t) => (t.id === track.id ? { ...t, decoding: false } : t)),
        );
      }
    }
  }, []);

  const removeTrack = useCallback((id: string) => {
    setTracks((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const updateTrack = useCallback((id: string, patch: Partial<TrackItem>) => {
    setTracks((prev) =>
      prev.map((t) => (t.id === id ? { ...t, ...patch } : t)),
    );
  }, []);

  // ── Drag-to-reorder ────────────────────────────────────────────────────────
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
      if (fi < 0 || ti < 0) return prev;
      const [item] = list.splice(fi, 1);
      list.splice(ti, 0, item);
      return list;
    });
    dragIdRef.current = null;
    dragOverIdRef.current = null;
  };

  // ── Join & Export ──────────────────────────────────────────────────────────
  const joinAndExport = useCallback(async () => {
    if (tracks.length < 2) {
      setError("Add at least 2 audio files to join.");
      return;
    }

    setStatus("joining");
    setProgress(0);
    setProgressLabel("Loading ffmpeg…");
    setError(null);
    if (outputUrl) URL.revokeObjectURL(outputUrl);
    setOutputUrl(null);

    const ffmpeg = await getFFmpeg();
    const jobId = uid("join");
    const codec = CODEC_FOR_FORMAT[outputFormat] ?? "libmp3lame";

    try {
      // Step 1 — write and normalize each track to an intermediate WAV.
      // Concat demuxer requires homogeneous stream params across all inputs.
      const processedNames: string[] = [];
      for (let i = 0; i < tracks.length; i++) {
        const track = tracks[i];
        const ext = getFileExtension(track.file.name) || "mp3";
        const rawName = `${jobId}_raw_${i}.${ext}`;
        const normalizedName = `${jobId}_n${i}.wav`;

        setProgressLabel(`Processing track ${i + 1} / ${tracks.length}…`);
        setProgress(Math.round((i / tracks.length) * 60));

        await ffmpeg.writeFile(rawName, await fetchFile(track.file));

        const hasTrim =
          track.trimStart > 0 ||
          (track.trimEnd !== null && track.trimEnd < track.duration - 0.05);

        const args = ["-i", rawName];
        if (hasTrim) {
          const trimEnd = track.trimEnd ?? track.duration;
          args.push("-ss", String(track.trimStart), "-to", String(trimEnd));
        }

        args.push(
          "-vn",
          "-ac",
          "2",
          "-ar",
          "48000",
          "-c:a",
          "pcm_s16le",
          normalizedName,
        );

        await runFFmpeg(ffmpeg, args);
        processedNames.push(normalizedName);
      }

      // Step 2 — build concat.txt
      setProgressLabel("Joining tracks…");
      setProgress(65);

      const concatContent = processedNames.map((n) => `file '${n}'`).join("\n");
      await ffmpeg.writeFile(
        "concat.txt",
        new TextEncoder().encode(concatContent),
      );

      const outputName = `joined_${jobId}.${outputFormat}`;

      let targetProgress = 65;
      const onProgress = ({ progress: r }: { progress: number }) => {
        targetProgress = Math.max(65, Math.min(95, 65 + Math.round(r * 30)));
      };
      let animRaf = 0;
      const animate = () => {
        setProgress((cur) =>
          Math.min(
            cur + Math.max(1, Math.round((targetProgress - cur) * 0.2)),
            targetProgress,
          ),
        );
        animRaf = requestAnimationFrame(animate);
      };
      ffmpeg.on("progress", onProgress);
      animRaf = requestAnimationFrame(animate);

      // Step 3 — concat
      await runFFmpeg(ffmpeg, [
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        "concat.txt",
        ...audioCodecArgs(codec),
        outputName,
      ]);

      cancelAnimationFrame(animRaf);
      ffmpeg.off("progress", onProgress);

      const out = await ffmpeg.readFile(outputName);
      const blob = new Blob([(out as Uint8Array).slice()], {
        type: `audio/${outputFormat}`,
      });
      setOutputUrl(URL.createObjectURL(blob));
      setOutputSize(blob.size);
      setProgress(100);
      setProgressLabel("Done!");
      setStatus("done");
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Join failed.";
      setError(msg);
      setStatus("error");
      setProgress(0);
    }
  }, [tracks, outputFormat, outputUrl]);

  const downloadOutput = useCallback(() => {
    if (!outputUrl) return;
    const a = document.createElement("a");
    a.href = outputUrl;
    a.download = `joined_audio.${outputFormat}`;
    a.click();
  }, [outputUrl, outputFormat]);

  const totalDuration = tracks.reduce((sum, t) => {
    const start = t.trimStart;
    const end = t.trimEnd ?? t.duration;
    return sum + Math.max(0, end - start);
  }, 0);

  // ── Render ─────────────────────────────────────────────────────────────────
  return (
    <div className="space-y-6">
      {/* Drop zone */}
      <FileDropZoneCard
        fileTypeLabel="audio files"
        supportedFormats="mp3, wav, flac, aac, m4a, opus, ogg"
        accept={ACCEPTED_AUDIO}
        multiple
        onFilesSelected={(files) => void addFiles(files)}
      />

      {/* Track list */}
      {tracks.length > 0 && (
        <div className="space-y-2">
          {tracks.map((track, index) => {
            const effectiveEnd = track.trimEnd ?? track.duration;
            const effectiveStart = track.trimStart;
            const hasTrim =
              effectiveStart > 0.01 ||
              (track.duration > 0 && effectiveEnd < track.duration - 0.05);
            const clipDuration =
              track.duration > 0 ? effectiveEnd - effectiveStart : 0;

            return (
              <div
                key={track.id}
                draggable
                onDragStart={() => handleDragStart(track.id)}
                onDragOver={(e) => handleDragOver(e, track.id)}
                onDrop={handleDrop}
                className="group rounded-2xl border border-white/10 bg-background/45 transition-colors hover:border-white/15"
              >
                {/* Main row */}
                <div className="flex items-center gap-3 p-3">
                  <GripVertical className="size-4 shrink-0 cursor-grab text-white/25 group-hover:text-white/50 active:cursor-grabbing" />
                  <span className="w-5 shrink-0 text-center text-xs text-muted-foreground">
                    {index + 1}
                  </span>

                  {/* Mini waveform */}
                  <div className="w-32 shrink-0 sm:w-44">
                    {track.decoding ? (
                      <div className="flex h-14 items-center justify-center rounded-xl border border-white/10 bg-black/30">
                        <Loader2 className="size-4 animate-spin text-muted-foreground" />
                      </div>
                    ) : track.audioBuffer ? (
                      <WaveformCanvas
                        audioBuffer={track.audioBuffer}
                        height={56}
                        trimHandles={false}
                      />
                    ) : (
                      <div className="flex h-14 items-center justify-center rounded-xl border border-white/10 bg-black/30 text-xs text-muted-foreground">
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
                          {hasTrim ? (
                            <>
                              <span className="text-info">
                                {formatDuration(clipDuration)}
                              </span>{" "}
                              <span className="text-white/30">
                                ({formatDuration(track.duration)})
                              </span>
                            </>
                          ) : (
                            formatDuration(track.duration)
                          )}
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Trim toggle */}
                  {track.audioBuffer && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className={`h-7 gap-1 border text-[10px] ${track.trimExpanded ? "border-info/25 bg-info/10 text-info" : "border-white/10 text-muted-foreground"}`}
                      onClick={() =>
                        updateTrack(track.id, {
                          trimExpanded: !track.trimExpanded,
                        })
                      }
                      disabled={status === "joining"}
                    >
                      {track.trimExpanded ? (
                        <ChevronUp className="size-3" />
                      ) : (
                        <ChevronDown className="size-3" />
                      )}
                      Trim
                    </Button>
                  )}

                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-8 shrink-0 text-muted-foreground"
                    onClick={() => removeTrack(track.id)}
                    disabled={status === "joining"}
                  >
                    <X className="size-4" />
                  </Button>
                </div>

                {/* ── Trim expand panel ── */}
                {track.trimExpanded && track.audioBuffer && (
                  <div className="border-t border-white/[0.06] px-3 pb-4 pt-3 space-y-3">
                    <WaveformCanvas
                      audioBuffer={track.audioBuffer}
                      height={100}
                      startRatio={
                        track.duration > 0 ? effectiveStart / track.duration : 0
                      }
                      endRatio={
                        track.duration > 0 ? effectiveEnd / track.duration : 1
                      }
                      onRegionChange={(s, e) =>
                        updateTrack(track.id, {
                          trimStart: parseFloat(
                            (s * track.duration).toFixed(2),
                          ),
                          trimEnd: parseFloat((e * track.duration).toFixed(2)),
                        })
                      }
                    />

                    {/* Time nudge row */}
                    <div className="flex flex-wrap items-center gap-4">
                      <div className="flex items-center gap-1.5">
                        <span className="text-xs text-muted-foreground">
                          Start
                        </span>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-6 rounded text-muted-foreground"
                          onClick={() =>
                            updateTrack(track.id, {
                              trimStart: parseFloat(
                                Math.max(0, effectiveStart - 0.1).toFixed(2),
                              ),
                            })
                          }
                        >
                          <Minus className="size-3" />
                        </Button>
                        <span className="w-16 text-center font-mono text-xs tabular-nums">
                          {formatDuration(effectiveStart)}
                        </span>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-6 rounded text-muted-foreground"
                          onClick={() =>
                            updateTrack(track.id, {
                              trimStart: parseFloat(
                                Math.min(
                                  effectiveEnd - 0.1,
                                  effectiveStart + 0.1,
                                ).toFixed(2),
                              ),
                            })
                          }
                        >
                          <Plus className="size-3" />
                        </Button>
                      </div>

                      <div className="flex items-center gap-1.5">
                        <span className="text-xs text-muted-foreground">
                          End
                        </span>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-6 rounded text-muted-foreground"
                          onClick={() =>
                            updateTrack(track.id, {
                              trimEnd: parseFloat(
                                Math.max(
                                  effectiveStart + 0.1,
                                  effectiveEnd - 0.1,
                                ).toFixed(2),
                              ),
                            })
                          }
                        >
                          <Minus className="size-3" />
                        </Button>
                        <span className="w-16 text-center font-mono text-xs tabular-nums">
                          {formatDuration(effectiveEnd)}
                        </span>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-6 rounded text-muted-foreground"
                          onClick={() =>
                            updateTrack(track.id, {
                              trimEnd: parseFloat(
                                Math.min(
                                  track.duration,
                                  effectiveEnd + 0.1,
                                ).toFixed(2),
                              ),
                            })
                          }
                        >
                          <Plus className="size-3" />
                        </Button>
                      </div>

                      {hasTrim && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 border border-white/10 px-2 text-[10px] text-muted-foreground"
                          onClick={() =>
                            updateTrack(track.id, {
                              trimStart: 0,
                              trimEnd: track.duration,
                            })
                          }
                        >
                          Reset
                        </Button>
                      )}

                      <span className="ml-auto font-mono text-xs text-muted-foreground">
                        {formatDuration(clipDuration)} clip
                      </span>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Empty state */}
      {tracks.length === 0 && (
        <Card className="flex flex-col items-center justify-center gap-2 border-white/10 bg-background/30 py-12 text-center">
          <ListMusic className="size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            Add two or more audio files to get started.
          </p>
        </Card>
      )}

      {/* Action bar */}
      {tracks.length >= 1 && (
        <Card className="divide-y divide-white/[0.06] border-white/10 bg-white/[0.015]">
          <div className="flex flex-wrap items-center gap-4 px-5 py-3">
            {totalDuration > 0 && (
              <p className="text-xs text-muted-foreground">
                Total output:{" "}
                <span className="font-mono">
                  {formatDuration(totalDuration)}
                </span>
                {" · "}
                {tracks.length} track{tracks.length !== 1 ? "s" : ""}
              </p>
            )}

            {progressLabel && status === "joining" && (
              <span className="text-xs text-muted-foreground">
                {progressLabel}
              </span>
            )}

            <div className="ml-auto flex items-center gap-2">
              <Select
                options={OUTPUT_FORMATS}
                value={outputFormat}
                onChange={(e) => setOutputFormat(e.target.value)}
                disabled={status === "joining"}
                className="h-8 w-28 border-white/10 bg-white/[0.04] text-xs shadow-none hover:bg-white/[0.07]"
              />

              {outputUrl && (
                <Button
                  variant="outline"
                  size="sm"
                  className="border-white/10 bg-white/[0.04] text-xs"
                  onClick={downloadOutput}
                >
                  <Download className="size-3.5" />
                  Download
                </Button>
              )}

              <Button
                size="sm"
                className="text-xs"
                disabled={status === "joining" || tracks.length < 2}
                onClick={() => void joinAndExport()}
              >
                {status === "joining" ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <RotateCcw className="size-3.5" />
                )}
                {status === "joining" ? "Joining…" : "Join & Export"}
              </Button>
            </div>
          </div>

          {(status === "joining" || status === "done") && (
            <div className="px-5 pb-4 pt-3 space-y-1">
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

      {error && <div className="error-banner">{error}</div>}
    </div>
  );
}
