"use client";

import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { drawWaveform } from "@/lib/audio/utils";

export interface WaveformCanvasProps {
  audioBuffer: AudioBuffer;
  startRatio?: number;
  endRatio?: number;
  playheadRatio?: number;
  onRegionChange?: (start: number, end: number) => void;
  height?: number;
  className?: string;
  trimHandles?: boolean;
}

const WAVEFORM_COLOR = "#b7b7b7ff";
const REGION_FILL = "rgba(102, 102, 102, 0.12)";
const HANDLE_COLOR = "#fdfdfd8e";
const HANDLE_WIDTH = 14;
const HANDLE_HIT_SLOP = 16;
const PLAYHEAD_COLOR = "rgba(255, 255, 255, 0.7)";
const DIM_FILL = "rgba(0, 0, 0, 0.35)";

type DragTarget = "start" | "end" | null;

export function WaveformCanvas({
  audioBuffer,
  startRatio,
  endRatio,
  playheadRatio,
  onRegionChange,
  height = 128,
  className = "",
  trimHandles = true,
}: WaveformCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const dragTarget = useRef<DragTarget>(null);
  const [canvasWidth, setCanvasWidth] = useState(800);

  const hasRegion =
    trimHandles &&
    startRatio !== undefined &&
    endRatio !== undefined &&
    onRegionChange !== undefined;

  useLayoutEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setCanvasWidth(Math.floor(entry.contentRect.width));
    });
    ro.observe(el);
    setCanvasWidth(Math.floor(el.getBoundingClientRect().width));
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const W = canvas.width;
    const H = canvas.height;

    ctx.clearRect(0, 0, W, H);

    drawWaveform(canvas, audioBuffer, WAVEFORM_COLOR);

    if (hasRegion && startRatio !== undefined && endRatio !== undefined) {
      const sx = startRatio * W;
      const ex = endRatio * W;

      ctx.fillStyle = DIM_FILL;
      ctx.fillRect(0, 0, sx, H);
      ctx.fillRect(ex, 0, W - ex, H);

      ctx.fillStyle = REGION_FILL;
      ctx.fillRect(sx, 0, ex - sx, H);

      const borderWidth = 2;
      ctx.fillStyle = HANDLE_COLOR;
      ctx.fillRect(sx, 0, ex - sx, borderWidth);
      ctx.fillRect(sx, H - borderWidth, ex - sx, borderWidth);

      const handlePadding = 2;
      const handleH = H - handlePadding * 2;

      for (const hx of [sx, ex]) {
        const cx = hx - HANDLE_WIDTH / 2;

        ctx.fillStyle = HANDLE_COLOR;
        ctx.beginPath();
        ctx.roundRect(cx, handlePadding, HANDLE_WIDTH, handleH, 6);
        ctx.fill();

        ctx.fillStyle = "rgba(0, 0, 0, 0.35)";
        const gripH = 24;
        const gripY = H / 2 - gripH / 2;
        ctx.fillRect(hx - 2.5, gripY, 1.5, gripH);
        ctx.fillRect(hx + 1.5, gripY, 1.5, gripH);
      }
    }

    if (playheadRatio !== undefined && playheadRatio >= 0) {
      const px = playheadRatio * W;
      ctx.strokeStyle = PLAYHEAD_COLOR;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(px, 0);
      ctx.lineTo(px, H);
      ctx.stroke();
    }
  }, [
    audioBuffer,
    canvasWidth,
    startRatio,
    endRatio,
    playheadRatio,
    hasRegion,
  ]);

  const ratioFromEvent = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      if (!canvas) return 0;
      const rect = canvas.getBoundingClientRect();
      return Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    },
    [],
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!hasRegion || startRatio === undefined || endRatio === undefined) {
        return;
      }
      const r = ratioFromEvent(e);
      const W = canvasRef.current?.getBoundingClientRect().width ?? 1;
      const slop = HANDLE_HIT_SLOP / W;

      if (Math.abs(r - startRatio) < slop) {
        dragTarget.current = "start";
      } else if (Math.abs(r - endRatio) < slop) {
        dragTarget.current = "end";
      } else {
        return;
      }
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      e.preventDefault();
    },
    [hasRegion, startRatio, endRatio, ratioFromEvent],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!dragTarget.current || !onRegionChange) return;
      if (startRatio === undefined || endRatio === undefined) return;
      const r = ratioFromEvent(e);
      e.preventDefault();

      if (dragTarget.current === "start") {
        onRegionChange(Math.min(r, endRatio - 0.01), endRatio);
      } else {
        onRegionChange(startRatio, Math.max(r, startRatio + 0.01));
      }
    },
    [onRegionChange, startRatio, endRatio, ratioFromEvent],
  );

  const handlePointerUp = useCallback(() => {
    dragTarget.current = null;
  }, []);

  const getCursor = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!hasRegion || startRatio === undefined || endRatio === undefined) {
        return;
      }
      const canvas = canvasRef.current;
      if (!canvas) return;
      const r = ratioFromEvent(e);
      const W = canvas.getBoundingClientRect().width;
      const slop = HANDLE_HIT_SLOP / W;
      const nearHandle =
        Math.abs(r - startRatio) < slop || Math.abs(r - endRatio) < slop;
      canvas.style.cursor = nearHandle ? "ew-resize" : "default";
    },
    [hasRegion, startRatio, endRatio, ratioFromEvent],
  );

  return (
    <div
      ref={wrapperRef}
      className={`relative w-full overflow-hidden rounded-2xl border border-white/10 bg-black/40 ${className}`}
      style={{ height }}
    >
      <canvas
        ref={canvasRef}
        width={canvasWidth}
        height={height}
        className="block h-full w-full touch-none"
        onPointerDown={handlePointerDown}
        onPointerMove={(e) => {
          getCursor(e);
          handlePointerMove(e);
        }}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onPointerLeave={handlePointerUp}
      />
    </div>
  );
}
