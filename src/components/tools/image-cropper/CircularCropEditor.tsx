"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";
import type {
  CropRect,
  Handle,
  PointerState,
  UploadedImage,
} from "@/lib/tools/image-cropper/types";
import {
  clampRect,
  getCenterAndRadiusFromBounds,
  loadImage,
  pointToImageSpace,
} from "@/lib/tools/image-cropper/utils";

interface CircularCropEditorProps {
  image: UploadedImage;
  crop: CropRect;
  onCropChange: (next: CropRect) => void;
}

export function CircularCropEditor({
  image,
  crop,
  onCropChange,
}: CircularCropEditorProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const [scale, setScale] = useState(1);
  const pointerStateRef = useRef<PointerState>({
    active: false,
    pointerId: null,
    handle: "move",
    startPoint: { x: 0, y: 0 },
    startRect: crop,
  });

  const { cx, cy, r } = getCenterAndRadiusFromBounds(crop);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const source = imageRef.current;
    if (!canvas || !source) return;

    const context = canvas.getContext("2d");
    if (!context) return;

    context.clearRect(0, 0, canvas.width, canvas.height);
    context.drawImage(source, 0, 0, canvas.width, canvas.height);

    const scaledCx = cx * scale;
    const scaledCy = cy * scale;
    const scaledR = r * scale;

    // Darkening overlay
    context.fillStyle = "rgba(0, 0, 0, 0.48)";
    context.fillRect(0, 0, canvas.width, canvas.height);

    // Clear circular area
    context.clearRect(
      scaledCx - scaledR,
      scaledCy - scaledR,
      scaledR * 2,
      scaledR * 2,
    );
    context.beginPath();
    context.arc(scaledCx, scaledCy, scaledR, 0, Math.PI * 2);
    context.clip();
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.resetTransform();

    // Draw circular outline
    context.strokeStyle = "#4ade80";
    context.lineWidth = 2;
    context.beginPath();
    context.arc(scaledCx, scaledCy, scaledR, 0, Math.PI * 2);
    context.stroke();

    // Draw center point
    context.fillStyle = "#4ade80";
    context.beginPath();
    context.arc(scaledCx, scaledCy, 4, 0, Math.PI * 2);
    context.fill();

    // Draw 4 radius handles (N, E, S, W)
    const handles = [
      [scaledCx, scaledCy - scaledR],
      [scaledCx + scaledR, scaledCy],
      [scaledCx, scaledCy + scaledR],
      [scaledCx - scaledR, scaledCy],
    ];

    handles.forEach(([x, y]) => {
      context.beginPath();
      context.arc(x, y, 4, 0, Math.PI * 2);
      context.fill();
    });
  }, [cx, cy, r, scale]);

  useEffect(() => {
    let mounted = true;

    void loadImage(image.srcUrl).then((source) => {
      if (!mounted) return;

      imageRef.current = source;
      const maxWidth = 720;
      const ratio = maxWidth / source.width;
      const nextScale = Math.min(1, ratio);
      setScale(nextScale);

      const canvas = canvasRef.current;
      if (!canvas) return;
      canvas.width = Math.round(source.width * nextScale);
      canvas.height = Math.round(source.height * nextScale);
      draw();
    });

    return () => {
      mounted = false;
    };
  }, [draw, image.srcUrl]);

  useEffect(() => {
    draw();
  }, [draw]);

  const updatePointer = useCallback(
    (event: React.PointerEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const point = pointToImageSpace(
        canvas,
        event.clientX,
        event.clientY,
        scale,
      );
      const pointer = pointerStateRef.current;
      const startCenterRadius = getCenterAndRadiusFromBounds(pointer.startRect);

      if (pointer.handle === "move") {
        const dx = point.x - pointer.startPoint.x;
        const dy = point.y - pointer.startPoint.y;
        const newCx = startCenterRadius.cx + dx;
        const newCy = startCenterRadius.cy + dy;
        const r = startCenterRadius.r;

        const newBounds = {
          x: newCx - r,
          y: newCy - r,
          w: r * 2,
          h: r * 2,
        };

        onCropChange(clampRect(newBounds, image.width, image.height));
        return;
      }

      // Handle radius adjustment
      const startCx = startCenterRadius.cx;
      const startCy = startCenterRadius.cy;
      const startR = startCenterRadius.r;
      const dx = point.x - pointer.startPoint.x;
      const dy = point.y - pointer.startPoint.y;

      let newR = startR;
      if (pointer.handle === "n") newR = startR - dy;
      else if (pointer.handle === "s") newR = startR + dy;
      else if (pointer.handle === "e") newR = startR + dx;
      else if (pointer.handle === "w") newR = startR - dx;

      newR = Math.max(
        10,
        Math.min(newR, Math.min(image.width, image.height) / 2),
      );

      const newBounds = {
        x: Math.round(startCx - newR),
        y: Math.round(startCy - newR),
        w: Math.round(newR * 2),
        h: Math.round(newR * 2),
      };

      onCropChange(clampRect(newBounds, image.width, image.height));
    },
    [image.height, image.width, onCropChange, scale],
  );

  const getHandleAtPoint = useCallback(
    (point: { x: number; y: number }, tolerance: number): Handle | null => {
      const handles: Array<{ name: Handle; x: number; y: number }> = [
        { name: "n", x: cx, y: cy - r },
        { name: "e", x: cx + r, y: cy },
        { name: "s", x: cx, y: cy + r },
        { name: "w", x: cx - r, y: cy },
      ];

      for (const candidate of handles) {
        if (
          Math.abs(point.x - candidate.x) <= tolerance &&
          Math.abs(point.y - candidate.y) <= tolerance
        ) {
          return candidate.name;
        }
      }

      const distToCenter = Math.sqrt(
        Math.pow(point.x - cx, 2) + Math.pow(point.y - cy, 2),
      );
      return distToCenter <= r ? "move" : null;
    },
    [cx, cy, r],
  );

  return (
    <canvas
      ref={canvasRef}
      className="max-w-full touch-none rounded-2xl border border-white/10 bg-black/30"
      onPointerDown={(event) => {
        const canvas = canvasRef.current;
        if (!canvas) return;

        const point = pointToImageSpace(
          canvas,
          event.clientX,
          event.clientY,
          scale,
        );
        const handle = getHandleAtPoint(point, 12 / scale) ?? "move";

        pointerStateRef.current = {
          active: true,
          pointerId: event.pointerId,
          handle,
          startPoint: point,
          startRect: crop,
        };

        canvas.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!pointerStateRef.current.active) return;
        updatePointer(event);
      }}
      onPointerUp={(event) => {
        const canvas = canvasRef.current;
        pointerStateRef.current.active = false;
        if (canvas?.hasPointerCapture(event.pointerId)) {
          canvas.releasePointerCapture(event.pointerId);
        }
      }}
      onPointerLeave={() => {
        pointerStateRef.current.active = false;
      }}
      onPointerCancel={() => {
        pointerStateRef.current.active = false;
      }}
    />
  );
}
