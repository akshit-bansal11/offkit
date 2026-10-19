"use client";

import { fetchFile } from "@ffmpeg/util";
import JSZip from "jszip";
import {
  ArrowRight,
  Clapperboard,
  Download,
  Loader2,
  Trash2,
  X,
} from "lucide-react";
import React, { useCallback, useEffect, useMemo, useState } from "react";
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
  ToolInputCardText,
  ToolInputCardTitle,
} from "@/components/common/ToolInputCard";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { Slider } from "@/components/ui/Slider";
import { getToolBySlug } from "@/config/tools";
import {
  audioCodecArgs,
  formatFileSize,
  getFFmpeg,
  getFileExtension,
  getVideoAudioCodecsForFormat,
  getVideoCodecsForFormat,
  runFFmpeg,
  stripExtension,
  uid,
  VIDEO_FORMAT_CODEC_MAP,
  type VideoOutputFormat,
} from "@/lib/ffmpeg/client";

const tool = getToolBySlug("video-converter");

export default function VideoConverterPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <VideoConverterTool />
    </ToolPageShell>
  );
}

type ConversionStatus = "idle" | "converting" | "done" | "error";

interface VideoItem {
  id: string;
  file: File;
  sourceFormat: string;
  targetFormat: VideoOutputFormat;
  videoCodec: string;
  audioCodec: string;
  resolution: string;
  quality: number;
  status: ConversionStatus;
  progress: number;
  error?: string;
  outputBlob?: Blob;
  outputUrl?: string;
}

const ACCEPTED_VIDEO = ".mp4,.webm,.mkv,.mov,.avi,.flv,.ogv,.3gp";
const VIDEO_FORMATS = Object.keys(
  VIDEO_FORMAT_CODEC_MAP,
) as VideoOutputFormat[];

function VideoConverterTool() {
  const [files, setFiles] = useState<VideoItem[]>([]);
  const [isConvertingAll, setIsConvertingAll] = useState(false);
  const [isDownloadingZip, setIsDownloadingZip] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const updateFile = useCallback((id: string, patch: Partial<VideoItem>) => {
    setFiles((prev) =>
      prev.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    );
  }, []);

  useEffect(() => {
    return () => {
      files.forEach((item) => {
        if (item.outputUrl) {
          URL.revokeObjectURL(item.outputUrl);
        }
      });
    };
  }, [files]);

  const addFiles = useCallback((incoming: File[]) => {
    const nextItems: VideoItem[] = incoming.map((file) => {
      const targetFormat: VideoOutputFormat = "mp4";
      const videoCodecs = getVideoCodecsForFormat(targetFormat);
      const audioCodecs = getVideoAudioCodecsForFormat(targetFormat);

      return {
        id: uid("video"),
        file,
        sourceFormat: getFileExtension(file.name) || "unknown",
        targetFormat,
        videoCodec: videoCodecs[0],
        audioCodec: audioCodecs[0],
        resolution: "original",
        quality: 23,
        status: "idle",
        progress: 0,
      };
    });

    setFiles((prev) => [...prev, ...nextItems]);
  }, []);

  const removeFile = useCallback((id: string) => {
    setFiles((prev) => {
      const item = prev.find((entry) => entry.id === id);
      if (item?.outputUrl) {
        URL.revokeObjectURL(item.outputUrl);
      }
      return prev.filter((entry) => entry.id !== id);
    });
  }, []);

  const convertOne = useCallback(
    async (item: VideoItem) => {
      updateFile(item.id, {
        status: "converting",
        progress: 0,
        error: undefined,
      });

      const ffmpeg = await getFFmpeg();
      const sourceExt = getFileExtension(item.file.name) || "bin";
      const inputName = `${item.id}_input.${sourceExt}`;
      const outputName = `${stripExtension(item.file.name)}_${item.id}.${item.targetFormat}`;

      let rafId = 0;
      let targetProgress = 0;

      const onProgress = ({ progress }: { progress: number }) => {
        targetProgress = Math.max(7, Math.min(96, Math.round(progress * 96)));
      };

      const animate = () => {
        setFiles((prev) =>
          prev.map((entry) => {
            if (entry.id !== item.id) {
              return entry;
            }

            if (entry.progress >= targetProgress) {
              return entry;
            }

            const delta = Math.max(
              1,
              Math.round((targetProgress - entry.progress) * 0.22),
            );
            return {
              ...entry,
              progress: Math.min(entry.progress + delta, targetProgress),
            };
          }),
        );
        rafId = requestAnimationFrame(animate);
      };

      ffmpeg.on("progress", onProgress);
      rafId = requestAnimationFrame(animate);

      try {
        await ffmpeg.writeFile(inputName, await fetchFile(item.file));
        const ffmpegArgs = ["-i", inputName];

        // Video codec & options
        if (item.videoCodec === "copy") {
          ffmpegArgs.push("-c:v", "copy");
        } else {
          ffmpegArgs.push("-c:v", item.videoCodec);

          // Apply resolution scaling
          if (item.resolution !== "original") {
            ffmpegArgs.push("-vf", `scale=-2:${item.resolution}`);
          }

          // Apply CRF (Quality) for encoders that support it
          if (["libx264", "libvpx"].includes(item.videoCodec)) {
            ffmpegArgs.push("-crf", String(item.quality));
          }
        }

        // Audio codec
        ffmpegArgs.push(...audioCodecArgs(item.audioCodec));

        ffmpegArgs.push(outputName);

        await runFFmpeg(ffmpeg, ffmpegArgs);

        const output = await ffmpeg.readFile(outputName);
        const blob = new Blob([(output as Uint8Array).slice()], {
          type: `video/${item.targetFormat}`,
        });
        const url = URL.createObjectURL(blob);

        if (item.outputUrl) {
          URL.revokeObjectURL(item.outputUrl);
        }

        updateFile(item.id, {
          status: "done",
          progress: 100,
          outputBlob: blob,
          outputUrl: url,
        });
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Video conversion failed.";
        updateFile(item.id, {
          status: "error",
          progress: 0,
          error: message,
        });
      } finally {
        cancelAnimationFrame(rafId);
        ffmpeg.off("progress", onProgress);
      }
    },
    [updateFile],
  );

  const convertAll = useCallback(async () => {
    setIsConvertingAll(true);

    const pending = files.filter(
      (item) => item.status === "idle" || item.status === "error",
    );
    for (const item of pending) {
      await convertOne(item);
    }

    setIsConvertingAll(false);
  }, [convertOne, files]);

  const downloadOne = useCallback((item: VideoItem) => {
    if (!item.outputUrl) {
      return;
    }

    const link = document.createElement("a");
    link.href = item.outputUrl;
    link.download = `${stripExtension(item.file.name)}.${item.targetFormat}`;
    link.click();
  }, []);

  const downloadAll = useCallback(async () => {
    const doneFiles = files.filter(
      (item) => item.status === "done" && item.outputBlob,
    );
    if (doneFiles.length === 0) {
      setErrorMessage("Convert at least one file before downloading ZIP.");
      return;
    }

    setIsDownloadingZip(true);
    setErrorMessage(null);

    try {
      const zip = new JSZip();
      doneFiles.forEach((item) => {
        if (item.outputBlob) {
          zip.file(
            `${stripExtension(item.file.name)}.${item.targetFormat}`,
            item.outputBlob,
          );
        }
      });

      const blob = await zip.generateAsync({ type: "blob" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "converted-videos.zip";
      link.click();
      URL.revokeObjectURL(url);
    } catch {
      setErrorMessage("Unable to create ZIP archive.");
    } finally {
      setIsDownloadingZip(false);
    }
  }, [files]);

  const doneCount = useMemo(
    () => files.filter((item) => item.status === "done").length,
    [files],
  );

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="video files"
        supportedFormats="mp4, webm, mkv, mov, avi, flv, ogv, and 3gp"
        accept={ACCEPTED_VIDEO}
        multiple
        onFilesSelected={(incoming) => {
          const valid = incoming.filter((file) => {
            const ext = getFileExtension(file.name);
            return (
              file.type.startsWith("video/") ||
              ACCEPTED_VIDEO.includes(`.${ext}`)
            );
          });

          if (valid.length > 0) {
            addFiles(valid);
          }
        }}
      />

      {files.length > 0 ? (
        <Card className="tool-card-inline">
          <CardHeader className="border-b border-white/10">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <CardTitle className="flex items-center gap-2 text-lg">
                <Clapperboard className="size-5 text-primary" />
                Video queue ({files.length})
              </CardTitle>
              <div className="flex flex-wrap gap-2">
                <Badge
                  variant="outline"
                  className="border-white/15 bg-white/[0.06] text-white/75"
                >
                  {doneCount} done
                </Badge>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void convertAll()}
                  disabled={isConvertingAll}
                >
                  {isConvertingAll ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : null}
                  Convert all
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void downloadAll()}
                  disabled={isDownloadingZip}
                >
                  {isDownloadingZip ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Download className="size-4" />
                  )}
                  Download ZIP
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    files.forEach((item) => {
                      if (item.outputUrl) {
                        URL.revokeObjectURL(item.outputUrl);
                      }
                    });
                    setFiles([]);
                  }}
                >
                  <Trash2 className="size-4" />
                  Clear
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4 p-5">
            {errorMessage ? (
              <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {errorMessage}
              </div>
            ) : null}

            {files.map((item) => {
              const videoCodecs = getVideoCodecsForFormat(item.targetFormat);
              const audioCodecs = getVideoAudioCodecsForFormat(
                item.targetFormat,
              );
              const outputSize = item.outputBlob
                ? formatFileSize(item.outputBlob.size)
                : null;
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
                  key={item.id}
                  tone={tone}
                  className="border-white/10 bg-background/40"
                >
                  <ToolInputCardInner className="space-y-4 p-4 pl-5">
                    <ToolInputCardHeader>
                      <div className="min-w-0 flex-1">
                        <ToolInputCardTitle>
                          {item.file.name}
                        </ToolInputCardTitle>
                        <ToolInputCardText className="mt-1 text-xs">
                          Source size: {formatFileSize(item.file.size)}
                          {outputSize ? ` -> Output size: ${outputSize}` : ""}
                        </ToolInputCardText>
                      </div>
                      <ToolInputCardDismissButton
                        onClick={() => removeFile(item.id)}
                      >
                        <X className="size-4" />
                      </ToolInputCardDismissButton>
                    </ToolInputCardHeader>

                    <ToolInputCardGrid className="lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)_minmax(0,1fr)]">
                      <ToolInputCardField>
                        <ToolInputCardLabel>Format</ToolInputCardLabel>
                        <div className="flex items-center gap-3">
                          <ToolInputCardBadge className="w-fit">
                            {item.sourceFormat}
                          </ToolInputCardBadge>
                          <ArrowRight className="size-4 shrink-0 text-muted-foreground" />
                          <div className="min-w-0 flex-1">
                            <ToolInputCardSelect
                              options={VIDEO_FORMATS.map((format) => ({
                                label: format.toUpperCase(),
                                value: format,
                              }))}
                              value={item.targetFormat}
                              onChange={(event) => {
                                const nextFormat = event.target
                                  .value as VideoOutputFormat;
                                const nextVideoCodecs =
                                  getVideoCodecsForFormat(nextFormat);
                                const nextAudioCodecs =
                                  getVideoAudioCodecsForFormat(nextFormat);
                                updateFile(item.id, {
                                  targetFormat: nextFormat,
                                  videoCodec: nextVideoCodecs[0],
                                  audioCodec: nextAudioCodecs[0],
                                  status: "idle",
                                  progress: 0,
                                  error: undefined,
                                });
                              }}
                              disabled={item.status === "converting"}
                            />
                          </div>
                        </div>
                      </ToolInputCardField>

                      <ToolInputCardField>
                        <ToolInputCardLabel>Codecs</ToolInputCardLabel>
                        <div className="grid gap-2 sm:grid-cols-2">
                          <ToolInputCardSelect
                            options={videoCodecs}
                            value={item.videoCodec}
                            onChange={(event) =>
                              updateFile(item.id, {
                                videoCodec: event.target.value,
                                status: "idle",
                                progress: 0,
                                error: undefined,
                              })
                            }
                            disabled={item.status === "converting"}
                          />
                          <ToolInputCardSelect
                            options={audioCodecs}
                            value={item.audioCodec}
                            onChange={(event) =>
                              updateFile(item.id, {
                                audioCodec: event.target.value,
                                status: "idle",
                                progress: 0,
                                error: undefined,
                              })
                            }
                            disabled={item.status === "converting"}
                          />
                        </div>
                      </ToolInputCardField>

                      <ToolInputCardField>
                        <ToolInputCardLabel>
                          Resolution & Quality
                        </ToolInputCardLabel>
                        <div className="flex items-center gap-3">
                          <ToolInputCardSelect
                            className="flex-1"
                            options={[
                              { label: "Original", value: "original" },
                              { label: "1080p", value: "1080" },
                              { label: "720p", value: "720" },
                              { label: "480p", value: "480" },
                              { label: "360p", value: "360" },
                            ]}
                            value={item.resolution}
                            onChange={(event) =>
                              updateFile(item.id, {
                                resolution: event.target.value,
                                status: "idle",
                                progress: 0,
                                error: undefined,
                              })
                            }
                            disabled={item.status === "converting"}
                          />
                          <div className="flex-1 space-y-1.5 rounded-md border border-white/10 bg-black/20 p-2">
                            <div className="flex items-center justify-between text-[10px] text-muted-foreground">
                              <span>CRF {item.quality}</span>
                              <span>
                                {item.quality < 20
                                  ? "High Quality"
                                  : item.quality > 35
                                    ? "Low Quality"
                                    : "Balanced"}
                              </span>
                            </div>
                            <Slider
                              min={0}
                              max={51}
                              step={1}
                              value={item.quality}
                              onChange={(event) =>
                                updateFile(item.id, {
                                  quality: Number(event.target.value),
                                  status: "idle",
                                  progress: 0,
                                  error: undefined,
                                })
                              }
                              disabled={
                                item.status === "converting" ||
                                item.videoCodec === "copy"
                              }
                            />
                          </div>
                        </div>
                      </ToolInputCardField>

                      <ToolInputCardField>
                        <ToolInputCardLabel>Actions</ToolInputCardLabel>
                        <div className="flex items-center gap-2">
                          <ToolInputCardButton
                            size="sm"
                            variant="outline"
                            className="flex-1"
                            disabled={item.status === "converting"}
                            onClick={() => void convertOne(item)}
                          >
                            {item.status === "converting" ? (
                              <Loader2 className="size-4 animate-spin" />
                            ) : null}
                            Convert
                          </ToolInputCardButton>
                          <ToolInputCardButton
                            size="sm"
                            className="flex-1"
                            disabled={item.status !== "done"}
                            onClick={() => downloadOne(item)}
                          >
                            <Download className="size-4" />
                            Save
                          </ToolInputCardButton>
                        </div>
                      </ToolInputCardField>
                    </ToolInputCardGrid>

                    <ToolInputCardField>
                      <ToolInputCardLabel>Progress</ToolInputCardLabel>
                      <div className="space-y-2">
                        <ToolInputCardProgress value={item.progress} />
                        <div className="flex flex-wrap items-center gap-2 text-xs">
                          <ToolInputCardBadge>{item.status}</ToolInputCardBadge>
                          {item.error ? (
                            <span className="text-destructive">
                              {item.error}
                            </span>
                          ) : null}
                        </div>
                      </div>
                    </ToolInputCardField>
                  </ToolInputCardInner>
                </ToolInputCard>
              );
            })}
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
