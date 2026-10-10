"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  clampRect,
  getHandleAtPoint,
  loadImage,
  pointToImageSpace,
  resizeRect,
} from "@/lib/tools/image-cropper/utils";
import type {
  CropRect,
  PointerState,
  UploadedImage,
} from "@/lib/tools/image-cropper/types";

interface CropCanvasEditorProps {
  image: UploadedImage;
  crop: CropRect;
  onCropChange: (next: CropRect) => void;
  aspectRatio: number | null;
}

export function CropCanvasEditor({
  image,
  crop,
  onCropChange,
  aspectRatio,
}: CropCanvasEditorProps) {
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

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const source = imageRef.current;
    if (!canvas || !source) return;

    const context = canvas.getContext("2d");
    if (!context) return;

    context.clearRect(0, 0, canvas.width, canvas.height);
    context.drawImage(source, 0, 0, canvas.width, canvas.height);

    const scaledCrop = {
      x: crop.x * scale,
      y: crop.y * scale,
      w: crop.w * scale,
      h: crop.h * scale,
    };

    context.fillStyle = "rgba(0, 0, 0, 0.48)";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.clearRect(scaledCrop.x, scaledCrop.y, scaledCrop.w, scaledCrop.h);

    context.strokeStyle = "#4ade80";
    context.lineWidth = 2;
    context.strokeRect(scaledCrop.x, scaledCrop.y, scaledCrop.w, scaledCrop.h);

    context.fillStyle = "#4ade80";
    [
      [scaledCrop.x, scaledCrop.y],
      [scaledCrop.x + scaledCrop.w / 2, scaledCrop.y],
      [scaledCrop.x + scaledCrop.w, scaledCrop.y],
      [scaledCrop.x + scaledCrop.w, scaledCrop.y + scaledCrop.h / 2],
      [scaledCrop.x + scaledCrop.w, scaledCrop.y + scaledCrop.h],
      [scaledCrop.x + scaledCrop.w / 2, scaledCrop.y + scaledCrop.h],
      [scaledCrop.x, scaledCrop.y + scaledCrop.h],
      [scaledCrop.x, scaledCrop.y + scaledCrop.h / 2],
    ].forEach(([x, y]) => {
      context.beginPath();
      context.arc(x, y, 4, 0, Math.PI * 2);
      context.fill();
    });
  }, [crop, scale]);

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

      if (pointer.handle === "move") {
        const dx = point.x - pointer.startPoint.x;
        const dy = point.y - pointer.startPoint.y;
        onCropChange(
          clampRect(
            {
              ...pointer.startRect,
              x: pointer.startRect.x + dx,
              y: pointer.startRect.y + dy,
            },
            image.width,
            image.height,
          ),
        );
        return;
      }

      onCropChange(
        clampRect(
          resizeRect(
            pointer.startRect,
            point.x - pointer.startPoint.x,
            point.y - pointer.startPoint.y,
            pointer.handle,
            aspectRatio,
          ),
          image.width,
          image.height,
        ),
      );
    },
    [aspectRatio, image.height, image.width, onCropChange, scale],
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
        const handle = getHandleAtPoint(point, crop, 12 / scale) ?? "move";

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
