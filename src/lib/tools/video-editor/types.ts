export type VideoTabId =
  | "trim"
  | "crop"
  | "rotate"
  | "resize"
  | "speed"
  | "volume"
  | "flip";
export type RotateAngle = 0 | 90 | 180 | 270;
export type VideoOutputFormat = "mp4" | "webm" | "mkv";

export interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface VideoEditorState {
  file: File | null;
  videoUrl: string | null;
  duration: number;
  status: "idle" | "loading" | "ready" | "exporting" | "done" | "error";
  error: string | null;
  progress: number;

  activeTab: VideoTabId;

  trimStart: number;
  trimEnd: number;

  cropX: number;
  cropY: number;
  cropW: number;
  cropH: number;
  cropEnabled: boolean;

  rotateAngle: RotateAngle;

  resolution: "original" | "2160" | "1080" | "720" | "480";
  customW: number;
  customH: number;
  aspectLock: boolean;

  speed: number;
  volume: number;

  flipH: boolean;
  flipV: boolean;

  outputFormat: VideoOutputFormat;
  outputUrl: string | null;
  outputSize: number;
}
