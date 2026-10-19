"use client";

import { fetchFile } from "@ffmpeg/util";
import { Download, Loader2, ScissorsLineDashed, X } from "lucide-react";
import React, { useCallback, useEffect, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Progress } from "@/components/ui/feedback/Progress";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import { getToolBySlug } from "@/config/tools";
import {
  type AudioOutputFormat,
  audioCodecArgs,
  formatFileSize,
  getAudioCodecsForFormat,
  getFFmpeg,
  getFileExtension,
  runFFmpeg,
  stripExtension,
  uid,
} from "@/lib/ffmpeg/client";

const tool = getToolBySlug("audio-extractor");

export default function AudioExtractorPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <AudioExtractorTool />
    </ToolPageShell>
  );
}

const ACCEPTED_VIDEO =
  "video/mp4,video/x-matroska,video/quicktime,video/x-msvideo,video/webm,video/x-flv";
const AUDIO_FORMATS = ["mp3", "aac", "opus", "flac", "wav", "m4a"] as const;

interface ExtractResult {
  blob: Blob;
  url: string;
  size: number;
}

function AudioExtractorTool() {
  const [inputFile, setInputFile] = useState<File | null>(null);
  const [targetFormat, setTargetFormat] =
    useState<(typeof AUDIO_FORMATS)[number]>("mp3");
  const [progress, setProgress] = useState(0);
  const [isExtracting, setIsExtracting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [result, setResult] = useState<ExtractResult | null>(null);

  const revokeResult = useCallback((nextResult: ExtractResult | null) => {
    if (nextResult?.url) {
      URL.revokeObjectURL(nextResult.url);
    }
  }, []);

  useEffect(() => {
    return () => {
      revokeResult(result);
    };
  }, [result, revokeResult]);

  const handleFile = useCallback(
    (file: File | null) => {
      if (!file) {
        return;
      }

      setErrorMessage(null);
      setInputFile(file);
      setProgress(0);
      revokeResult(result);
      setResult(null);
    },
    [result, revokeResult],
  );

  const onFormatChange = useCallback(
    (nextFormat: (typeof AUDIO_FORMATS)[number]) => {
      setTargetFormat(nextFormat);
    },
    [],
  );

  const extractAudio = useCallback(async () => {
    if (!inputFile) {
      setErrorMessage("Upload a video file to extract audio.");
      return;
    }

    setIsExtracting(true);
    setErrorMessage(null);
    setProgress(0);

    const ffmpeg = await getFFmpeg();
    const jobId = uid("extract");
    const sourceExt = getFileExtension(inputFile.name) || "bin";
    const inputName = `${jobId}_input.${sourceExt}`;
    const outputName = `${stripExtension(inputFile.name)}_audio.${targetFormat}`;
    const codec = getAudioCodecsForFormat(targetFormat as AudioOutputFormat)[0];
    let rafId = 0;
    let targetProgress = 0;

    const onProgress = ({ progress: ratio }: { progress: number }) => {
      targetProgress = Math.max(8, Math.min(95, Math.round(ratio * 95)));
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
      await ffmpeg.writeFile(inputName, await fetchFile(inputFile));
      await runFFmpeg(ffmpeg, [
        "-i",
        inputName,
        "-vn",
        ...audioCodecArgs(codec),
        outputName,
      ]);

      const output = await ffmpeg.readFile(outputName);
      const blob = new Blob([(output as Uint8Array).slice()], {
        type: `audio/${targetFormat}`,
      });
      const url = URL.createObjectURL(blob);

      revokeResult(result);
      setProgress(100);
      setResult({ blob, url, size: blob.size });
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : "Unable to extract audio from this file.";
      setErrorMessage(message);
      setProgress(0);
    } finally {
      cancelAnimationFrame(rafId);
      ffmpeg.off("progress", onProgress);
      setIsExtracting(false);
    }
  }, [inputFile, result, revokeResult, targetFormat]);

  const download = useCallback(() => {
    if (!result || !inputFile) {
      return;
    }

    const link = document.createElement("a");
    link.href = result.url;
    link.download = `${stripExtension(inputFile.name)}_audio.${targetFormat}`;
    link.click();
  }, [inputFile, result, targetFormat]);

  const sourceExt = inputFile ? getFileExtension(inputFile.name) : "";

  return (
    <div className="space-y-6">
      <FileDropZoneCard
        fileTypeLabel="a video file"
        supportedFormats="mp4, mkv, mov, avi, webm, and flv"
        accept={ACCEPTED_VIDEO}
        onFilesSelected={(incoming) => {
          const file = incoming[0] ?? null;
          if (!file) {
            return;
          }

          const isAcceptedMime = ACCEPTED_VIDEO.split(",").includes(file.type);
          if (!isAcceptedMime) {
            setErrorMessage(
              "Unsupported video type. Please upload mp4, mkv, mov, avi, webm, or flv.",
            );
            return;
          }

          handleFile(file);
        }}
      />

      {inputFile ? (
        <div className="rounded-xl border border-white/10 bg-background/40 p-4">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <p className="font-medium">{inputFile.name}</p>
              <p className="text-xs text-muted-foreground">
                {formatFileSize(inputFile.size)}
              </p>
            </div>
            <Badge
              variant="outline"
              className="border-white/15 bg-background/70"
            >
              {sourceExt.toUpperCase()}
            </Badge>
          </div>

          <div className="grid gap-3">
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground">Output format</p>
              <Select
                options={AUDIO_FORMATS.map((format) => ({
                  label: format.toUpperCase(),
                  value: format,
                }))}
                className="h-10 surface-inset border-white/[0.06] bg-white/[0.06] text-foreground shadow-none hover:bg-white/[0.09] focus:ring-0 focus:ring-offset-0"
                value={targetFormat}
                onChange={(event) =>
                  onFormatChange(
                    event.target.value as (typeof AUDIO_FORMATS)[number],
                  )
                }
                disabled={isExtracting}
              />
            </div>
          </div>

          <div className="mt-4 flex flex-wrap gap-2">
            <Button disabled={isExtracting} onClick={() => void extractAudio()}>
              {isExtracting ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <ScissorsLineDashed className="size-4" />
              )}
              Extract audio
            </Button>
            <Button variant="outline" disabled={!result} onClick={download}>
              <Download className="size-4" />
              Download
            </Button>
            <Button
              variant="ghost"
              disabled={isExtracting}
              onClick={() => {
                setInputFile(null);
                setProgress(0);
                setErrorMessage(null);
                revokeResult(result);
                setResult(null);
              }}
            >
              <X className="size-4" />
              Reset
            </Button>
          </div>

          <div className="mt-4 space-y-2">
            <Progress value={progress} />
            {result ? (
              <p className="text-xs text-white/70">
                Extracted size: {formatFileSize(result.size)}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}

      {errorMessage ? (
        <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {errorMessage}
        </div>
      ) : null}
    </div>
  );
}
