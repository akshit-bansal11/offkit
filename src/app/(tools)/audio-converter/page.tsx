"use client";

import { getToolBySlug } from "@/config/tools";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import React, { useCallback, useEffect, useMemo, useState } from "react";
import { fetchFile } from "@ffmpeg/util";
import JSZip from "jszip";
import { ArrowRight, Download, Loader2, Music, Trash2, X } from "lucide-react";
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
  ToolInputCardPreview,
  ToolInputCardProgress,
  ToolInputCardSelect,
  ToolInputCardText,
  ToolInputCardTitle,
} from "@/components/common/ToolInputCard";
import { Badge } from "@/components/ui/feedback/Badge";
import { Button } from "@/components/ui/Button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import {
  AUDIO_FORMAT_CODEC_MAP,
  type AudioOutputFormat,
  formatFileSize,
  audioCodecArgs,
  getAudioCodecsForFormat,
  getFFmpeg,
  runFFmpeg,
  getFileExtension,
  stripExtension,
  uid,
} from "@/lib/ffmpeg/client";

const tool = getToolBySlug("audio-converter");

export default function AudioConverterPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <AudioConverterTool />
    </ToolPageShell>
  );
}


type ConversionStatus = "idle" | "converting" | "done" | "error";

interface AudioFileItem {
  id: string;
  file: File;
  sourceFormat: string;
  targetFormat: AudioOutputFormat;
  status: ConversionStatus;
  progress: number;
  error?: string;
  outputBlob?: Blob;
  outputUrl?: string;
}

const ACCEPTED_AUDIO =
  ".mp3,.opus,.m4a,.aac,.mpeg,.mpg,.wma,.flac,.wav,.aiff,.aif";
const AUDIO_FORMATS = Object.keys(
  AUDIO_FORMAT_CODEC_MAP,
) as AudioOutputFormat[];

const AUDIO_FORMAT_OPTIONS = AUDIO_FORMATS.map((format) => ({
  label: format.toUpperCase(),
  value: format,
}));

function animateNumber(
  start: number,
  end: number,
  durationMs: number,
  onFrame: (value: number) => void,
): Promise<void> {
  return new Promise((resolve) => {
    const startTime = performance.now();

    const tick = (now: number) => {
      const elapsed = now - startTime;
      const progress = Math.min(elapsed / durationMs, 1);
      const nextValue = Math.round(start + (end - start) * progress);
      onFrame(nextValue);

      // Timers, not animation frames: this promise is awaited by the
      // conversion, and frames never fire while the tab is in the background.
      if (progress < 1) {
        setTimeout(() => tick(performance.now()), 16);
      } else {
        resolve();
      }
    };

    tick(startTime);
  });
}

function AudioConverterTool() {
  const [files, setFiles] = useState<AudioFileItem[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isConvertingAll, setIsConvertingAll] = useState(false);
  const [isDownloadingZip, setIsDownloadingZip] = useState(false);

  const updateFile = useCallback(
    (id: string, patch: Partial<AudioFileItem>) => {
      setFiles((prev) =>
        prev.map((item) => (item.id === id ? { ...item, ...patch } : item)),
      );
    },
    [],
  );

  const revokeOutputUrl = useCallback((item: AudioFileItem) => {
    if (item.outputUrl) {
      URL.revokeObjectURL(item.outputUrl);
    }
  }, []);

  useEffect(() => {
    return () => {
      files.forEach((item) => revokeOutputUrl(item));
    };
  }, [files, revokeOutputUrl]);

  const addFiles = useCallback((incoming: File[]) => {
    const nextItems: AudioFileItem[] = incoming.map((file) => {
      const sourceFormat = getFileExtension(file.name) || "unknown";
      const targetFormat: AudioOutputFormat = "mp3";
      return {
        id: uid("audio"),
        file,
        sourceFormat,
        targetFormat,
        status: "idle",
        progress: 0,
      };
    });

    setFiles((prev) => [...prev, ...nextItems]);
  }, []);

  const removeFile = useCallback(
    (id: string) => {
      setFiles((prev) => {
        const target = prev.find((item) => item.id === id);
        if (target) {
          revokeOutputUrl(target);
        }
        return prev.filter((item) => item.id !== id);
      });
    },
    [revokeOutputUrl],
  );

  const clearAll = useCallback(() => {
    files.forEach((item) => revokeOutputUrl(item));
    setFiles([]);
  }, [files, revokeOutputUrl]);

  const convertOne = useCallback(
    async (item: AudioFileItem) => {
      if (item.status === "converting") {
        return;
      }

      updateFile(item.id, {
        status: "converting",
        progress: 0,
        error: undefined,
      });

      const ffmpeg = await getFFmpeg();
      const inputName = `${item.id}_input.${item.sourceFormat || "bin"}`;
      const outputName = `${stripExtension(item.file.name)}_${item.id}.${item.targetFormat}`;
      const progressState = { target: 0, rafId: 0 };

      const smoothTick = () => {
        setFiles((prev) =>
          prev.map((entry) => {
            if (entry.id !== item.id) {
              return entry;
            }

            const next =
              entry.progress +
              Math.max(
                1,
                Math.round((progressState.target - entry.progress) * 0.2),
              );
            const clamped = Math.min(next, progressState.target, 99);
            return { ...entry, progress: clamped };
          }),
        );

        progressState.rafId = requestAnimationFrame(smoothTick);
      };

      const onProgress = ({ progress }: { progress: number }) => {
        const computed = Math.max(8, Math.min(96, Math.round(progress * 96)));
        progressState.target = computed;
      };

      ffmpeg.on("progress", onProgress);
      progressState.rafId = requestAnimationFrame(smoothTick);

      try {
        await ffmpeg.writeFile(inputName, await fetchFile(item.file));
        await animateNumber(0, 10, 250, (value) =>
          updateFile(item.id, { progress: value }),
        );

        await runFFmpeg(ffmpeg, [
          "-i",
          inputName,
          "-vn",
          ...audioCodecArgs(getAudioCodecsForFormat(item.targetFormat)[0]),
          outputName,
        ]);
        progressState.target = 99;

        const output = await ffmpeg.readFile(outputName);
        const outBlob = new Blob([(output as Uint8Array).slice()], {
          type: `audio/${item.targetFormat}`,
        });

        if (item.outputUrl) {
          URL.revokeObjectURL(item.outputUrl);
        }

        const outUrl = URL.createObjectURL(outBlob);

        updateFile(item.id, {
          status: "done",
          progress: 100,
          outputBlob: outBlob,
          outputUrl: outUrl,
        });
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Audio conversion failed.";
        updateFile(item.id, {
          status: "error",
          progress: 0,
          error: message,
        });
      } finally {
        cancelAnimationFrame(progressState.rafId);
        ffmpeg.off("progress", onProgress);
      }
    },
    [updateFile],
  );

  const convertAll = useCallback(async () => {
    setIsConvertingAll(true);
    setErrorMessage(null);

    const pending = files.filter(
      (item) => item.status === "idle" || item.status === "error",
    );

    for (const item of pending) {
      await convertOne(item);
    }

    setIsConvertingAll(false);
  }, [convertOne, files]);

  const downloadOne = useCallback((item: AudioFileItem) => {
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
      setErrorMessage("Convert at least one file before downloading a ZIP.");
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

      const zipBlob = await zip.generateAsync({ type: "blob" });
      const zipUrl = URL.createObjectURL(zipBlob);
      const link = document.createElement("a");
      link.href = zipUrl;
      link.download = "converted-audio.zip";
      link.click();
      URL.revokeObjectURL(zipUrl);
    } catch {
      setErrorMessage("Unable to package converted files as ZIP.");
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
        fileTypeLabel="audio files"
        supportedFormats="mp3, opus, m4a, aac, mpeg, wma, flac, wav, and aiff"
        accept={ACCEPTED_AUDIO}
        multiple
        onFilesSelected={(incoming) => {
          const validFiles = incoming.filter(
            (file) =>
              file.type.startsWith("audio/") ||
              ACCEPTED_AUDIO.includes(`.${getFileExtension(file.name)}`),
          );

          if (validFiles.length > 0) {
            addFiles(validFiles);
          }
        }}
      />

      {files.length > 0 && (
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-4">
              <CardTitle className="flex items-center gap-2 text-2xl">
                Conversion queue
                <Badge variant="outline" className="badge-emerald text-sm">
                  {doneCount} done
                </Badge>
              </CardTitle>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={isConvertingAll}
                  onClick={() => void convertAll()}
                >
                  {isConvertingAll ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : null}
                  Convert all
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={isDownloadingZip}
                  onClick={() => void downloadAll()}
                >
                  {isDownloadingZip ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Download className="size-4" />
                  )}
                  Download ZIP
                </Button>
                <Button variant="ghost" size="sm" onClick={clearAll}>
                  <Trash2 className="size-4" />
                  Clear
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            {errorMessage ? (
              <div className="error-banner">{errorMessage}</div>
            ) : null}

            <div className="space-y-4">
              {files.map((item) => {
                const isBusy = item.status === "converting";
                const isDone = item.status === "done";
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
                    className="surface-inset rounded-2xl"
                  >
                    <ToolInputCardInner>
                      <ToolInputCardHeader>
                        <div className="min-w-0 flex-1">
                          <ToolInputCardTitle>
                            {item.file.name}
                          </ToolInputCardTitle>
                          <ToolInputCardText className="mt-1 text-xs">
                            {formatFileSize(item.file.size)}
                          </ToolInputCardText>
                        </div>
                        <ToolInputCardDismissButton
                          onClick={() => removeFile(item.id)}
                        >
                          <X className="size-4" />
                        </ToolInputCardDismissButton>
                      </ToolInputCardHeader>

                      <ToolInputCardGrid>
                        <ToolInputCardField>
                          <ToolInputCardLabel>Format</ToolInputCardLabel>
                          <div className="flex items-center gap-3">
                            <ToolInputCardBadge>
                              {item.sourceFormat || "unknown"}
                            </ToolInputCardBadge>
                            <ArrowRight className="size-3.5 shrink-0 text-muted-foreground" />
                            <div className="min-w-0 flex-1">
                              <ToolInputCardSelect
                                options={AUDIO_FORMAT_OPTIONS}
                                value={item.targetFormat}
                                onChange={(
                                  event: React.ChangeEvent<HTMLSelectElement>,
                                ) => {
                                  const nextFormat = event.target
                                    .value as AudioOutputFormat;
                                  updateFile(item.id, {
                                    targetFormat: nextFormat,
                                    status: "idle",
                                    progress: 0,
                                    error: undefined,
                                  });
                                }}
                                disabled={isBusy}
                              />
                            </div>
                          </div>
                        </ToolInputCardField>

                        <ToolInputCardField>
                          <ToolInputCardLabel>Progress</ToolInputCardLabel>
                          <div className="space-y-2">
                            <ToolInputCardProgress value={item.progress} />
                            <div className="flex items-center justify-between gap-3 text-xs">
                              <ToolInputCardBadge>
                                {item.status === "converting"
                                  ? `${item.progress}%`
                                  : item.status}
                              </ToolInputCardBadge>
                              {item.error ? (
                                <span className="text-right text-red-400">
                                  {item.error}
                                </span>
                              ) : null}
                            </div>
                          </div>
                        </ToolInputCardField>

                        <ToolInputCardField>
                          <ToolInputCardLabel>Actions</ToolInputCardLabel>
                          <div className="flex items-center gap-2">
                            <ToolInputCardButton
                              size="sm"
                              variant="secondary"
                              className="flex-1"
                              disabled={isBusy}
                              onClick={() => void convertOne(item)}
                            >
                              {isBusy ? (
                                <Loader2 className="size-4 animate-spin" />
                              ) : (
                                <Music className="size-4" />
                              )}
                              Convert
                            </ToolInputCardButton>
                            <ToolInputCardButton
                              size="sm"
                              className="flex-1"
                              disabled={!isDone}
                              onClick={() => downloadOne(item)}
                            >
                              <Download className="size-4" />
                              Save
                            </ToolInputCardButton>
                          </div>
                        </ToolInputCardField>
                      </ToolInputCardGrid>

                      {isDone && item.outputUrl ? (
                        <ToolInputCardPreview>
                          <ToolInputCardLabel className="mb-3">
                            Preview
                          </ToolInputCardLabel>
                          <div className="flex justify-center">
                            <audio
                              controls
                              src={item.outputUrl}
                              className="w-full max-w-sm rounded-lg"
                            />
                          </div>
                        </ToolInputCardPreview>
                      ) : null}
                    </ToolInputCardInner>
                  </ToolInputCard>
                );
              })}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

