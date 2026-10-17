"use client";

import JSZip from "jszip";
import { Crop, Download, Loader2, Scissors, X } from "lucide-react";
import NextImage from "next/image";
import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { CircularCropEditor } from "@/components/tools/image-cropper/CircularCropEditor";
import { CropCanvasEditor } from "@/components/tools/image-cropper/CropCanvasEditor";
import { MeshCropEditor } from "@/components/tools/image-cropper/MeshCropEditor";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Select } from "@/components/ui/form/Select";
import { FileDropZoneCard } from "@/components/ui/interaction/FileDropZoneCard";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { getToolBySlug } from "@/config/tools";
import { formatFileSize, uid } from "@/lib/ffmpeg/client";
import {
  ACCEPTED_IMAGES,
  ASPECT_PRESETS,
  type AspectValue,
} from "@/lib/tools/image-cropper/constants";
import type {
  CropMode,
  CropRect,
  CropShape,
  UploadedImage,
} from "@/lib/tools/image-cropper/types";
import {
  clampBatchCropToImage,
  createDefaultCircularCrop,
  createDefaultCrop,
  cropToBlob,
  cropToCircularBlob,
  fitCropToAspect,
  getAspectRatio,
  loadImage,
} from "@/lib/tools/image-cropper/utils";

const tool = getToolBySlug("image-cropper");

export default function ImageCropperPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ImageCropperTool />
    </ToolPageShell>
  );
}

function ImageCropperTool() {
  const [images, setImages] = useState<UploadedImage[]>([]);
  const [mode, setMode] = useState<CropMode>("individual");
  const [cropShape, setCropShape] = useState<CropShape>("rectangular");
  const [aspect, setAspect] = useState<AspectValue>("free");
  const [batchCrop, setBatchCrop] = useState<CropRect | null>(null);
  const [isDownloadingZip, setIsDownloadingZip] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const aspectRatio = useMemo(() => getAspectRatio(aspect), [aspect]);

  // Revoke on unmount only. Keying this cleanup on `images` revoked every
  // source URL on the first state update, breaking all later previews.
  const imagesRef = useRef(images);
  useEffect(() => {
    imagesRef.current = images;
  }, [images]);
  useEffect(() => {
    return () => {
      imagesRef.current.forEach((image) => {
        URL.revokeObjectURL(image.srcUrl);
        if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);
      });
    };
  }, []);

  const addFiles = useCallback(
    async (incoming: File[]) => {
      const loaded: UploadedImage[] = [];

      for (const file of incoming) {
        if (!file.type.startsWith("image/")) continue;

        const srcUrl = URL.createObjectURL(file);

        try {
          const source = await loadImage(srcUrl);
          const crop =
            cropShape === "circular"
              ? createDefaultCircularCrop(
                  source.naturalWidth,
                  source.naturalHeight,
                )
              : createDefaultCrop(source.naturalWidth, source.naturalHeight);

          loaded.push({
            id: uid("crop"),
            file,
            name: file.name,
            type: file.type,
            size: file.size,
            srcUrl,
            width: source.naturalWidth,
            height: source.naturalHeight,
            crop,
            cropShape,
          });
        } catch {
          URL.revokeObjectURL(srcUrl);
          setErrorMessage("One or more selected images could not be read.");
        }
      }

      if (loaded.length === 0) return;

      setImages((prev) => [...prev, ...loaded]);
      if (!batchCrop) setBatchCrop(loaded[0].crop);
      setErrorMessage(null);
    },
    [batchCrop, cropShape],
  );

  const updateImage = useCallback(
    (id: string, patch: Partial<UploadedImage>) => {
      setImages((prev) =>
        prev.map((image) => (image.id === id ? { ...image, ...patch } : image)),
      );
    },
    [],
  );

  const removeImage = useCallback((id: string) => {
    setImages((prev) => {
      const target = prev.find((image) => image.id === id);
      if (target) {
        URL.revokeObjectURL(target.srcUrl);
        if (target.outputUrl) URL.revokeObjectURL(target.outputUrl);
      }
      return prev.filter((image) => image.id !== id);
    });
  }, []);

  const clearAll = useCallback(() => {
    images.forEach((image) => {
      URL.revokeObjectURL(image.srcUrl);
      if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);
    });

    setImages([]);
    setBatchCrop(null);
    setErrorMessage(null);
  }, [images]);

  const getPngFileName = (originalName: string) => {
    return originalName.replace(/\.[^.]+$/, ".png");
  };

  const generatePreviewForImage = useCallback(
    async (image: UploadedImage) => {
      const crop =
        mode === "batch" && batchCrop
          ? clampBatchCropToImage(batchCrop, image)
          : image.crop;

      const blob =
        image.cropShape === "circular"
          ? await cropToCircularBlob(image, crop)
          : await cropToBlob(image, crop);

      const outputUrl = URL.createObjectURL(blob);

      if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);

      updateImage(image.id, { outputBlob: blob, outputUrl });
    },
    [batchCrop, mode, updateImage],
  );

  const previewAll = useCallback(async () => {
    for (const image of images) {
      await generatePreviewForImage(image);
    }
  }, [generatePreviewForImage, images]);

  const downloadOne = useCallback((image: UploadedImage) => {
    if (!image.outputUrl) return;

    const link = document.createElement("a");
    link.href = image.outputUrl;
    link.download = getPngFileName(image.name);
    link.click();
  }, []);

  const downloadAll = useCallback(async () => {
    setIsDownloadingZip(true);
    setErrorMessage(null);

    try {
      const zip = new JSZip();

      for (const image of images) {
        let blob = image.outputBlob;
        if (!blob) {
          const crop =
            mode === "batch" && batchCrop
              ? clampBatchCropToImage(batchCrop, image)
              : image.crop;
          blob =
            image.cropShape === "circular"
              ? await cropToCircularBlob(image, crop)
              : await cropToBlob(image, crop);
        }

        const pngName = getPngFileName(image.name);
        zip.file(pngName, blob);
      }

      const zipBlob = await zip.generateAsync({ type: "blob" });
      const zipUrl = URL.createObjectURL(zipBlob);
      const link = document.createElement("a");
      link.href = zipUrl;
      link.download = "cropped-images.zip";
      link.click();
      URL.revokeObjectURL(zipUrl);
    } catch {
      setErrorMessage("Failed to prepare ZIP download for cropped images.");
    } finally {
      setIsDownloadingZip(false);
    }
  }, [batchCrop, images, mode]);

  const setImageCrop = useCallback((id: string, next: CropRect) => {
    setImages((prev) =>
      prev.map((image) => {
        if (image.id !== id) return image;
        if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);

        return {
          ...image,
          crop: next,
          outputBlob: undefined,
          outputUrl: undefined,
        };
      }),
    );
  }, []);

  const changeCropShape = useCallback(
    (newShape: CropShape) => {
      setCropShape(newShape);
      setImages((prev) =>
        prev.map((image) => {
          const crop =
            newShape === "circular"
              ? createDefaultCircularCrop(image.width, image.height)
              : createDefaultCrop(image.width, image.height);

          if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);

          return {
            ...image,
            cropShape: newShape,
            crop,
            outputBlob: undefined,
            outputUrl: undefined,
          };
        }),
      );
      const first = images[0];
      setBatchCrop(
        first
          ? newShape === "circular"
            ? createDefaultCircularCrop(first.width, first.height)
            : createDefaultCrop(first.width, first.height)
          : null,
      );
    },
    [images],
  );

  const changeAspect = useCallback((next: AspectValue) => {
    setAspect(next);
    const ratio = getAspectRatio(next);
    if (!ratio) return;

    setImages((prev) =>
      prev.map((image) => {
        if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);
        return {
          ...image,
          crop: fitCropToAspect(image.crop, ratio, image.width, image.height),
          outputBlob: undefined,
          outputUrl: undefined,
        };
      }),
    );
    setBatchCrop((prev) => {
      const first = imagesRef.current[0];
      return prev && first
        ? fitCropToAspect(prev, ratio, first.width, first.height)
        : prev;
    });
  }, []);

  const referenceImage = images[0] ?? null;

  return (
    <div className="space-y-6">
      <Card className="tool-card-inline">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-lg">
            <Crop className="size-5 text-primary" />
            Multi-image crop editor
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="space-y-3">
            <div>
              <p className="mb-2 text-xs text-muted-foreground">
                Processing mode
              </p>
              <SegmentedControl
                variant="dark"
                size="sm"
                value={mode}
                onValueChange={(value) => setMode(value as CropMode)}
                options={[
                  { label: "Individual Crop", value: "individual" },
                  { label: "Batch Crop", value: "batch" },
                ]}
              />
            </div>

            <div>
              <p className="mb-2 text-xs text-muted-foreground">Crop mode</p>
              <SegmentedControl
                variant="dark"
                size="sm"
                value={cropShape}
                onValueChange={(value) => changeCropShape(value as CropShape)}
                options={[
                  { label: "Rectangular", value: "rectangular" },
                  { label: "Circular", value: "circular" },
                  { label: "Mesh", value: "mesh" },
                ]}
              />
            </div>
          </div>

          <div className="grid gap-3 md:grid-cols-[1fr_auto] md:items-end">
            {cropShape === "rectangular" && (
              <div className="space-y-2">
                <p className="text-xs text-muted-foreground">
                  Aspect ratio preset
                </p>
                <Select
                  options={ASPECT_PRESETS.map((preset) => ({
                    label: preset.label,
                    value: preset.value,
                  }))}
                  value={aspect}
                  onChange={(event) =>
                    changeAspect(event.target.value as AspectValue)
                  }
                />
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                onClick={() => void previewAll()}
                disabled={images.length === 0}
              >
                <Scissors className="size-4" />
                Preview all
              </Button>
              <Button
                variant="outline"
                onClick={() => void downloadAll()}
                disabled={images.length === 0 || isDownloadingZip}
              >
                {isDownloadingZip ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Download className="size-4" />
                )}
                Download ZIP
              </Button>
              <Button
                variant="ghost"
                onClick={clearAll}
                disabled={images.length === 0}
              >
                <X className="size-4" />
                Clear
              </Button>
            </div>
          </div>

          <FileDropZoneCard
            fileTypeLabel="image files"
            supportedFormats="JPG, PNG, and WEBP"
            accept={ACCEPTED_IMAGES}
            multiple
            onFilesSelected={(files) => {
              void addFiles(files);
            }}
          />

          {errorMessage ? (
            <div className="rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-sm text-red-200">
              {errorMessage}
            </div>
          ) : null}
        </CardContent>
      </Card>

      {mode === "batch" && referenceImage && batchCrop ? (
        <Card className="tool-card-inline">
          <CardHeader className="border-b border-white/10">
            <CardTitle className="text-lg">Shared batch crop region</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 p-5">
            <p className="text-sm text-muted-foreground">
              Batch crop is edited on the first uploaded image and clamped to
              each image during export.
            </p>
            {cropShape === "rectangular" && (
              <CropCanvasEditor
                image={referenceImage}
                crop={batchCrop}
                aspectRatio={aspectRatio}
                onCropChange={(next) => {
                  setBatchCrop(next);
                  setImages((prev) =>
                    prev.map((image) => {
                      if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);
                      return {
                        ...image,
                        outputBlob: undefined,
                        outputUrl: undefined,
                      };
                    }),
                  );
                }}
              />
            )}
            {cropShape === "circular" && (
              <CircularCropEditor
                image={referenceImage}
                crop={batchCrop}
                onCropChange={(next) => {
                  setBatchCrop(next);
                  setImages((prev) =>
                    prev.map((image) => {
                      if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);
                      return {
                        ...image,
                        outputBlob: undefined,
                        outputUrl: undefined,
                      };
                    }),
                  );
                }}
              />
            )}
            {cropShape === "mesh" && (
              <MeshCropEditor
                image={referenceImage}
                crop={batchCrop}
                aspectRatio={aspectRatio}
                onCropChange={(next) => {
                  setBatchCrop(next);
                  setImages((prev) =>
                    prev.map((image) => {
                      if (image.outputUrl) URL.revokeObjectURL(image.outputUrl);
                      return {
                        ...image,
                        outputBlob: undefined,
                        outputUrl: undefined,
                      };
                    }),
                  );
                }}
              />
            )}
          </CardContent>
        </Card>
      ) : null}

      {images.length > 0 ? (
        <Card className="tool-card-inline">
          <CardHeader className="border-b border-white/10">
            <CardTitle className="text-lg">
              Image list ({images.length})
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4 p-5">
            {images.map((image) => {
              const activeCrop =
                mode === "batch" && batchCrop
                  ? clampBatchCropToImage(batchCrop, image)
                  : image.crop;

              return (
                <Card
                  key={image.id}
                  className="border-white/10 bg-background/40"
                >
                  <CardContent className="space-y-4 p-4">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div>
                        <p className="font-medium">{image.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {image.width} x {image.height} px •{" "}
                          {formatFileSize(image.size)}
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge
                          variant="outline"
                          className="border-white/15 bg-background/70"
                        >
                          {Math.round(activeCrop.w)} x{" "}
                          {Math.round(activeCrop.h)}
                        </Badge>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => removeImage(image.id)}
                        >
                          <X className="size-4" />
                        </Button>
                      </div>
                    </div>

                    {mode === "individual" ? (
                      <>
                        {cropShape === "rectangular" && (
                          <CropCanvasEditor
                            image={image}
                            crop={image.crop}
                            aspectRatio={aspectRatio}
                            onCropChange={(next) =>
                              setImageCrop(image.id, next)
                            }
                          />
                        )}
                        {cropShape === "circular" && (
                          <CircularCropEditor
                            image={image}
                            crop={image.crop}
                            onCropChange={(next) =>
                              setImageCrop(image.id, next)
                            }
                          />
                        )}
                        {cropShape === "mesh" && (
                          <MeshCropEditor
                            image={image}
                            crop={image.crop}
                            aspectRatio={aspectRatio}
                            onCropChange={(next) =>
                              setImageCrop(image.id, next)
                            }
                          />
                        )}
                      </>
                    ) : (
                      <div className="grid gap-3 sm:grid-cols-[220px_1fr] sm:items-center">
                        <NextImage
                          src={image.srcUrl}
                          alt={image.name}
                          width={220}
                          height={128}
                          className="h-32 w-full rounded-lg border border-white/10 object-cover"
                          unoptimized
                        />
                        <p className="text-sm text-muted-foreground">
                          Batch crop will use this image with clamped bounds at
                          export time.
                        </p>
                      </div>
                    )}

                    <div className="flex flex-wrap gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => void generatePreviewForImage(image)}
                      >
                        <Scissors className="size-4" />
                        Preview crop
                      </Button>
                      <Button
                        size="sm"
                        disabled={!image.outputUrl}
                        onClick={() => downloadOne(image)}
                      >
                        <Download className="size-4" />
                        Download
                      </Button>
                    </div>

                    {image.outputUrl ? (
                      <div className="rounded-lg border border-white/10 bg-black/35 p-2">
                        <NextImage
                          src={image.outputUrl}
                          alt={`${image.name} cropped preview`}
                          width={320}
                          height={240}
                          className="max-h-60 w-auto rounded-md"
                          unoptimized
                        />
                      </div>
                    ) : null}
                  </CardContent>
                </Card>
              );
            })}
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
