// client.ts
// FFmpeg WASM client loader, specific format codecs, and file utilities.

import { FFmpeg } from "@ffmpeg/ffmpeg";
import { toBlobURL } from "@ffmpeg/util";

// Copied from @ffmpeg/core by scripts/copy-vendor.mjs.
const CORE_BASE_URL = "/vendor/ffmpeg";

let ffmpegInstance: FFmpeg | null = null;
let ffmpegLoadPromise: Promise<FFmpeg> | null = null;

async function loadCore(ffmpeg: FFmpeg) {
  const coreURL = await toBlobURL(
    `${CORE_BASE_URL}/ffmpeg-core.js`,
    "text/javascript",
  );
  const wasmURL = await toBlobURL(
    `${CORE_BASE_URL}/ffmpeg-core.wasm`,
    "application/wasm",
  );

  await ffmpeg.load({ coreURL, wasmURL });
}

export async function getFFmpeg(): Promise<FFmpeg> {
  if (ffmpegInstance) {
    return ffmpegInstance;
  }

  if (!ffmpegLoadPromise) {
    ffmpegLoadPromise = (async () => {
      const ffmpeg = new FFmpeg();
      await loadCore(ffmpeg);
      ffmpegInstance = ffmpeg;
      return ffmpeg;
    })();
  }

  return ffmpegLoadPromise;
}

// A wasm abort leaves the module unusable, so the next job must get a fresh one.
function resetFFmpeg(ffmpeg: FFmpeg) {
  try {
    ffmpeg.terminate();
  } catch {
    // Already terminated.
  }
  if (ffmpegInstance === ffmpeg) {
    ffmpegInstance = null;
    ffmpegLoadPromise = null;
  }
}

/**
 * Run an ffmpeg command and throw a readable error when it fails.
 * `exec` resolves with a non-zero exit code on ordinary failures and only
 * rejects when the wasm module itself aborts, so both paths are handled here.
 */
export async function runFFmpeg(ffmpeg: FFmpeg, args: string[]): Promise<void> {
  const tail: string[] = [];
  const onLog = ({ message }: { message: string }) => {
    tail.push(message);
    if (tail.length > 30) tail.shift();
  };

  ffmpeg.on("log", onLog);

  let exitCode: number;
  try {
    exitCode = await ffmpeg.exec(args);
  } catch {
    resetFFmpeg(ffmpeg);
    throw new Error(
      "The media engine crashed on this file and was restarted. Try again, or pick a different output format.",
    );
  } finally {
    ffmpeg.off("log", onLog);
  }

  if (exitCode !== 0) {
    throw new Error(describeFFmpegFailure(tail));
  }
}

export function describeFFmpegFailure(logTail: readonly string[]): string {
  const reason = [...logTail]
    .reverse()
    .find(
      (line) =>
        /error|invalid|unsupported|unable|could not|does not contain|no such/i.test(
          line,
        ) && !/^(Conversion failed!|Aborted\(\))/.test(line.trim()),
    );

  return reason
    ? `Processing failed: ${reason.replace(/^\[[^\]]+\]\s*/, "").trim()}`
    : "Processing failed. This file or format combination is not supported.";
}

/**
 * Output arguments for an audio encoder. The bundled libopus crashes on
 * common stereo 44.1 kHz input, so Opus uses ffmpeg's native encoder, which
 * is flagged experimental and only accepts 48 kHz mono or stereo.
 */
export function audioCodecArgs(codec: string): string[] {
  return codec === "opus"
    ? ["-c:a", "opus", "-strict", "-2", "-ar", "48000", "-ac", "2"]
    : ["-c:a", codec];
}

export const AUDIO_FORMAT_CODEC_MAP = {
  mp3: ["libmp3lame"],
  opus: ["opus"],
  m4a: ["aac"],
  aac: ["aac"],
  mpeg: ["mp2", "libmp3lame"],
  wma: ["wmav2"],
  flac: ["flac"],
  wav: ["pcm_s16le", "pcm_s24le", "pcm_f32le"],
  aiff: ["pcm_s16be", "pcm_s24be"],
} as const;

export type AudioOutputFormat = keyof typeof AUDIO_FORMAT_CODEC_MAP;

export function getAudioCodecsForFormat(
  format: AudioOutputFormat,
): readonly string[] {
  return AUDIO_FORMAT_CODEC_MAP[format];
}

// Only encoders verified to finish in the single-threaded @ffmpeg/core build.
// libx265 hangs and libvpx-vp9 aborts there; libxvid is not compiled in.
export const VIDEO_FORMAT_CODEC_MAP = {
  mp4: ["libx264", "mpeg4"],
  webm: ["libvpx"],
  mkv: ["libx264", "libvpx", "mpeg4"],
  mov: ["libx264", "prores", "mpeg4"],
  avi: ["mpeg4", "libx264"],
  flv: ["libx264", "flv"],
  ogv: ["libtheora"],
  "3gp": ["libx264", "mpeg4"],
} as const;

export type VideoOutputFormat = keyof typeof VIDEO_FORMAT_CODEC_MAP;

export function getVideoCodecsForFormat(
  format: VideoOutputFormat,
): readonly string[] {
  return VIDEO_FORMAT_CODEC_MAP[format];
}

const VIDEO_AUDIO_CODEC_MAP: Record<VideoOutputFormat, readonly string[]> = {
  mp4: ["aac", "libmp3lame"],
  mkv: ["aac", "libmp3lame", "libvorbis", "copy"],
  mov: ["aac", "libmp3lame"],
  webm: ["libvorbis", "opus"],
  avi: ["aac", "libmp3lame"],
  flv: ["aac", "libmp3lame"],
  ogv: ["libvorbis"],
  "3gp": ["aac"],
};

export function getVideoAudioCodecsForFormat(
  format: VideoOutputFormat,
): readonly string[] {
  return VIDEO_AUDIO_CODEC_MAP[format];
}

/** Default encoders for a video container, as ffmpeg output arguments. */
export function videoContainerCodecArgs(format: VideoOutputFormat): string[] {
  return [
    "-c:v",
    VIDEO_FORMAT_CODEC_MAP[format][0],
    ...audioCodecArgs(VIDEO_AUDIO_CODEC_MAP[format][0]),
  ];
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function getFileExtension(name: string): string {
  const parts = name.toLowerCase().split(".");
  return parts.length > 1 ? parts[parts.length - 1] : "";
}

export function stripExtension(name: string): string {
  return name.replace(/\.[^/.]+$/, "");
}

export function uid(prefix: string): string {
  return `${prefix}_${Math.random().toString(36).slice(2, 9)}`;
}
