// tools.ts
// Tool registry: all tool definitions, categories, and derived lookup helpers.

import {
  ArrowLeftRight,
  AudioLines,
  Binary,
  Blend,
  Box,
  Code2,
  Crop,
  Droplet,
  FileCode2,
  FileDiff,
  FileEdit,
  FileInput,
  FileJson,
  FileOutput,
  FileText,
  Film,
  FolderArchive,
  FolderOpen,
  FolderTree,
  Grid3X3,
  ImageIcon,
  ImageUp,
  Layers,
  LibraryBig,
  ListMusic,
  ListVideo,
  Merge,
  Minimize2,
  Palette,
  Pipette,
  Scissors,
  Shapes,
  SlidersHorizontal,
  Sparkles,
  SplitSquareHorizontal,
  SwatchBook,
  Type,
  VectorSquare,
  Video,
  Wand2,
} from "lucide-react";
import type { Metadata } from "next";
import type { ToolCategoryDefinition, ToolDefinition } from "@/types/tool";
// Relative, not @/, so the Node test runner can load this module.
import { siteConfig } from "./site.ts";

export const toolCategories: ToolCategoryDefinition[] = [
  { name: "Audio", icon: AudioLines, accentColor: "#4aa8ff" },
  { name: "Video", icon: Video, accentColor: "#ffc23d" },
  { name: "Dev", icon: Code2, accentColor: "#2fd59a" },
  { name: "Images", icon: ImageIcon, accentColor: "#ff5a4e" },
  { name: "SVG", icon: VectorSquare, accentColor: "#f472b6" },
  { name: "Colors", icon: SwatchBook, accentColor: "#a78bfa" },
  { name: "CSS", icon: FileCode2, accentColor: "#fb923c" },
  { name: "PDF", icon: FileText, accentColor: "#34d399" },
  { name: "Text", icon: Type, accentColor: "#4aa8ff" },
];

export const tools: ToolDefinition[] = [
  {
    slug: "audio-converter",
    platforms: ["web", "desktop"],
    href: "/audio-converter",
    name: "Audio Converter",
    shortName: "Convert",
    description:
      "Convert audio files between formats and codecs entirely on your device using FFmpeg.",
    category: "Audio",
    highlights: [
      "Per-file format + codec controls",
      "Batch conversion with ZIP downloads",
    ],
    icon: ArrowLeftRight,
  },
  {
    slug: "audio-extractor",
    platforms: ["web", "desktop"],
    href: "/audio-extractor",
    name: "Audio Extractor",
    shortName: "Extract",
    description:
      "Extract the audio track from any video file and download it in your chosen format and codec.",
    category: "Audio",
    highlights: ["Video to audio extraction", "Codec-aware output selection"],
    icon: Scissors,
  },
  {
    slug: "audio-editor",
    platforms: ["web", "desktop"],
    href: "/audio-editor",
    name: "Audio Editor",
    shortName: "Edit",
    description:
      "Trim, adjust speed, shift pitch, and apply EQ to any audio file, all on your device.",
    category: "Audio",
    highlights: ["Waveform trim handles", "Speed, pitch & EQ controls"],
    icon: SlidersHorizontal,
  },
  {
    slug: "audio-joiner",
    platforms: ["web", "desktop"],
    href: "/audio-joiner",
    name: "Audio Joiner",
    shortName: "Join",
    description:
      "Drag-and-drop multiple audio files and join them into a single file in your chosen format.",
    category: "Audio",
    highlights: ["Drag-to-reorder tracks", "ffmpeg concat demuxer"],
    icon: ListMusic,
  },
  {
    slug: "video-converter",
    platforms: ["web", "desktop"],
    href: "/video-converter",
    name: "Video Converter",
    shortName: "Convert",
    description:
      "Convert videos between formats and codecs directly on your device using FFmpeg.",
    category: "Video",
    highlights: [
      "Per-file video and audio codec mapping",
      "Batch conversion and ZIP export",
    ],
    icon: ArrowLeftRight,
  },
  {
    slug: "frames-extractor",
    platforms: ["web", "desktop"],
    href: "/frames-extractor",
    name: "Frames Extractor",
    shortName: "Frames",
    description:
      "Upload a GIF or video and extract every frame as individual PNG images, then download them as a ZIP.",
    category: "Video",
    highlights: ["Frame thumbnail grid", "FPS and frame-count metadata"],
    icon: Layers,
  },
  {
    slug: "directory-scaffold",
    platforms: ["web", "desktop"],
    href: "/directory-scaffold",
    name: "Directory Scaffold",
    shortName: "Scaffold",
    description:
      "Turn a folder or a public GitHub repo into an ASCII or Markdown tree.",
    category: "Dev",
    highlights: ["Collapsible file tree UI", "ASCII tree copy and TXT export"],
    icon: FolderTree,
  },
  {
    slug: "image-cropper",
    platforms: ["web", "desktop"],
    href: "/image-cropper",
    name: "Image Cropper",
    shortName: "Crop",
    description:
      "Crop one or many images on your device - apply individual crops per image or a single crop to all at once.",
    category: "Images",
    highlights: [
      "Individual and batch crop modes",
      "Canvas drag-resize crop handles",
    ],
    icon: Crop,
  },
  {
    slug: "svg-animator",
    platforms: ["web", "desktop"],
    href: "/svg-animator",
    name: "SVG Border Animator",
    shortName: "Animate",
    description:
      "Upload any SVG and animate each path border locally with stroke-dashoffset controls and export-ready output.",
    category: "SVG",
    highlights: [
      "Sequential or simultaneous path animation",
      "Downloadable SVG with embedded keyframes",
    ],
    icon: Wand2,
  },
  {
    slug: "svg-pattern",
    platforms: ["web", "desktop"],
    href: "/svg-pattern",
    name: "SVG Pattern Generator",
    shortName: "Pattern",
    description:
      "Create beautiful, scalable vector patterns directly on your device. Export native CSS backgrounds or raw SVG data URIs instantly.",
    category: "SVG",
    highlights: [
      "Injected vector properties directly generated",
      "CSS backgrounds",
    ],
    icon: Grid3X3,
  },
  {
    slug: "blob-generator",
    platforms: ["web", "desktop"],
    href: "/blob-generator",
    name: "CSS Blob Generator",
    shortName: "Blob",
    description:
      "Create intricate, organic shapes natively by independently customizing the 8 anchor points of the border-radius property.",
    category: "CSS",
    highlights: ["8-Point interpolators natively computed"],
    icon: Shapes,
  },
  {
    slug: "glassmorphism",
    platforms: ["web", "desktop"],
    href: "/glassmorphism",
    name: "Glassmorphism Generator",
    shortName: "Glass",
    description:
      "Design stunning frosted-glass UI elements with real-time backdrop filtering and export the CSS directly.",
    category: "CSS",
    highlights: ["Interactive refractive blur previews", "Tailwind & CSS"],
    icon: Droplet,
  },
  {
    slug: "box-shadow",
    platforms: ["web", "desktop"],
    href: "/box-shadow",
    name: "Box Shadow Generator",
    shortName: "Shadow",
    description:
      "Create layered CSS shadows natively through visual controls and export the exact styling string for your projects.",
    category: "CSS",
    highlights: ["Detailed multi-layer controls natively"],
    icon: Box,
  },
  {
    slug: "pdf-toolkit",
    platforms: ["web", "desktop"],
    href: "/pdf-toolkit",
    name: "PDF Toolkit",
    shortName: "Toolkit",
    description:
      "Merge multiple files, break PDFs apart by page, reorder structure, or compress files directly on your device.",
    category: "PDF",
    highlights: ["Compress documents securely natively"],
    icon: FileText,
  },
  {
    slug: "lorem-generator",
    platforms: ["web", "desktop"],
    href: "/lorem-generator",
    name: "Lorem Generator",
    shortName: "Lorem",
    description:
      "Generate customized placeholder text instantly for your mockups, using either classic Latin or random English prose.",
    category: "Text",
    highlights: ["Configurable words, sentences, and paragraphs"],
    icon: Type,
  },
  {
    slug: "diff-checker",
    platforms: ["web", "desktop"],
    href: "/diff-checker",
    name: "Diff Checker",
    shortName: "Diff",
    description:
      "Find inline character-level or line-level text differences with robust ignoring whitespace and case sensitivity.",
    category: "Text",
    highlights: ["Unified patch and raw markup generation"],
    icon: FileDiff,
  },
  {
    slug: "svg-optimizer",
    platforms: ["web", "desktop"],
    href: "/svg-optimizer",
    name: "SVG Optimizer",
    shortName: "Optimize",
    description:
      "Clean up messy SVGs, strip metadata, dial in decimal precision, and convert to JSX directly on your device.",
    category: "SVG",
    highlights: ["Live visual preview with JSX conversion"],
    icon: Sparkles,
  },
  {
    slug: "pdf-converter",
    platforms: ["web", "desktop"],
    href: "/pdf-converter",
    name: "PDF ↔ Image Converter",
    shortName: "Convert",
    description:
      "Merge images into a single PDF, batch convert them, or extract pages from existing PDFs as images.",
    category: "PDF",
    highlights: ["Drag-to-reorder combined page layout"],
    icon: ArrowLeftRight,
  },
  {
    slug: "image-converter",
    platforms: ["web", "desktop"],
    href: "/image-converter",
    name: "Image Converter",
    shortName: "Convert",
    description:
      "Convert popular image formats locally with batch downloads, quality controls, and upload guardrails.",
    category: "Images",
    highlights: ["Batch conversion with ZIP downloads"],
    icon: ArrowLeftRight,
  },
  {
    slug: "json-formatter",
    platforms: ["web", "desktop"],
    href: "/json-formatter",
    name: "JSON Formatter",
    shortName: "Format",
    description:
      "Pretty-print, minify, validate, and copy JSON on your device.",
    category: "Text",
    highlights: ["Pretty print or minify in one click"],
    icon: FileJson,
  },
  {
    slug: "gradient-maker",
    platforms: ["web", "desktop"],
    href: "/gradient-maker",
    name: "Gradient Maker",
    shortName: "Gradients",
    description:
      "Build multi-stop gradients with live previews, PNG exports, CSS output, and Tailwind arbitrary values.",
    category: "Colors",
    highlights: ["Drag and reorder gradient stops"],
    icon: Blend,
  },
  {
    slug: "gradient-library",
    platforms: ["web", "desktop"],
    href: "/gradient-library",
    name: "Gradient Library",
    shortName: "Library",
    description:
      "Browse curated named gradients and copy them as CSS or Tailwind-ready arbitrary values.",
    category: "Colors",
    highlights: ["Curated presets with instant PNG downloads"],
    icon: LibraryBig,
  },
  {
    slug: "palette-library",
    platforms: ["web", "desktop"],
    href: "/palette-library",
    name: "Palette Library",
    shortName: "Library",
    description:
      "Explore curated color palettes, copy individual swatches, or export full palettes as JSON.",
    category: "Colors",
    highlights: ["Copy any swatch or full palette JSON"],
    icon: Palette,
  },
  {
    slug: "palette-extractor",
    platforms: ["web", "desktop"],
    href: "/palette-extractor",
    name: "Palette Extractor",
    shortName: "Extract",
    description:
      "Upload an image and extract its dominant colors into a copyable palette, computed locally on your device.",
    category: "Colors",
    highlights: ["On-device dominant color extraction"],
    icon: ImageUp,
  },
  {
    slug: "gradient-converter",
    platforms: ["web", "desktop"],
    href: "/gradient-converter",
    name: "Gradient Converter",
    shortName: "Convert",
    description:
      "Convert CSS gradients into Tailwind arbitrary values or switch between linear, radial, and conic syntax.",
    category: "Colors",
    highlights: ["Live preview while converting gradient syntax"],
    icon: ArrowLeftRight,
  },
  {
    slug: "color-converter",
    platforms: ["web", "desktop"],
    href: "/color-converter",
    name: "Color Converter",
    shortName: "Convert",
    description:
      "Convert HEX, RGB, HSL, HSV, OKLCH, and named colors into every other format at once.",
    category: "Colors",
    highlights: ["All major color models at the same time"],
    icon: Pipette,
  },
  {
    slug: "base64",
    platforms: ["web", "desktop"],
    href: "/base64",
    name: "Base64",
    shortName: "Base64",
    description:
      "Encode and decode text with a clean local workflow that stays entirely on your device.",
    category: "Text",
    highlights: ["Unicode-safe encode and decode"],
    icon: Binary,
  },

  {
    slug: "video-editor",
    platforms: ["web", "desktop"],
    href: "/video-editor",
    name: "Video Editor",
    shortName: "Edit",
    description:
      "Trim, crop, rotate, resize, flip, adjust speed and volume for any video, entirely on your device.",
    category: "Video",
    highlights: ["Trim, crop, rotate & resize", "Speed & volume adjustment"],
    icon: Film,
  },
  {
    slug: "video-merger",
    platforms: ["web", "desktop"],
    href: "/video-merger",
    name: "Video Merger",
    shortName: "Merge",
    description:
      "Drag-and-drop multiple video files, reorder them, and merge into a single video.",
    category: "Video",
    highlights: ["Drag-to-reorder clips", "Resolution normalization"],
    icon: ListVideo,
  },

  {
    slug: "pdf-merger",
    platforms: ["web", "desktop"],
    href: "/pdf-merger",
    name: "PDF Merger",
    shortName: "Merge",
    description:
      "Combine multiple PDF documents into one file with drag-to-reorder.",
    category: "PDF",
    highlights: ["Drag-to-reorder pages", "Per-file page count"],
    icon: Merge,
  },
  {
    slug: "pdf-splitter",
    platforms: ["web", "desktop"],
    href: "/pdf-splitter",
    name: "PDF Splitter",
    shortName: "Split",
    description:
      "Break a PDF into individual pages or extract specific page ranges.",
    category: "PDF",
    highlights: ["Visual page picker", "Fixed-size chunk splitting"],
    icon: SplitSquareHorizontal,
  },
  {
    slug: "pdf-compressor",
    platforms: ["web", "desktop"],
    href: "/pdf-compressor",
    name: "PDF Compressor",
    shortName: "Compress",
    description:
      "Reduce PDF file size by rasterizing pages at adjustable quality and resolution.",
    category: "PDF",
    highlights: [
      "Adjustable render scale & quality",
      "Size comparison preview",
    ],
    icon: Minimize2,
  },
  {
    slug: "convert-to-pdf",
    platforms: ["web", "desktop"],
    href: "/convert-to-pdf",
    name: "Convert to PDF",
    shortName: "To PDF",
    description:
      "Convert images to PDF. Drop multiple images and combine them into a single document.",
    category: "PDF",
    highlights: ["Drag-to-reorder images", "Fit, A4, or Letter page sizes"],
    icon: FileOutput,
  },
  {
    slug: "convert-from-pdf",
    platforms: ["web", "desktop"],
    href: "/convert-from-pdf",
    name: "Convert from PDF",
    shortName: "From PDF",
    description:
      "Extract PDF pages as PNG or JPEG images and download them as a ZIP.",
    category: "PDF",
    highlights: ["Configurable render scale", "PNG & JPEG output"],
    icon: FileInput,
  },

  {
    slug: "md-editor",
    platforms: ["web", "desktop"],
    href: "/md-editor",
    name: "Markdown Editor",
    shortName: "Markdown",
    description:
      "Write Markdown with a live split-pane preview, toolbar shortcuts, and export to .md or PDF.",
    category: "Text",
    highlights: ["Live split-pane preview", "Toolbar shortcuts & PDF export"],
    icon: FileEdit,
  },

  {
    slug: "archive-extractor",
    platforms: ["web", "desktop"],
    href: "/archive-extractor",
    name: "Archive Extractor",
    shortName: "Extract",
    description:
      "Upload a ZIP archive, preview its file tree, and extract individual files or download everything.",
    category: "Dev",
    highlights: ["Interactive file tree", "Extract individual files or all"],
    icon: FolderOpen,
  },
  {
    slug: "archiver",
    platforms: ["web", "desktop"],
    href: "/archiver",
    name: "Archiver",
    shortName: "Archive",
    description: "Drop files to create a ZIP archive instantly on your device.",
    category: "Dev",
    highlights: ["Editable file paths", "DEFLATE compression"],
    icon: FolderArchive,
  },
];

export function getToolBySlug(slug: string) {
  return tools.find((tool) => tool.slug === slug);
}

/**
 * Route metadata for a tool page: its own title, description, canonical URL
 * and social card text, all from the registry entry. Relative URLs resolve
 * against metadataBase in the root layout. An unknown slug fails the build.
 */
export function getToolMetadata(slug: string): Metadata {
  const tool = getToolBySlug(slug);
  if (!tool) throw new Error(`No tool is registered with the slug "${slug}".`);

  return {
    title: tool.name,
    description: tool.description,
    alternates: { canonical: tool.href },
    openGraph: {
      title: tool.name,
      description: tool.description,
      url: tool.href,
      images: [siteConfig.socialImage],
    },
  };
}

export const toolsByCategory = toolCategories.map((category) => ({
  ...category,
  tools: tools.filter((tool) => tool.category === category.name),
}));

export function getToolAccentColor(slug: string): string {
  const tool = getToolBySlug(slug);
  if (!tool) return "#4aa8ff";
  const category = toolCategories.find((c) => c.name === tool.category);
  return category?.accentColor ?? "#4aa8ff";
}

/** True when the tool runs in the browser. A desktop-only tool keeps its URL but shows a download panel. */
export function runsOnWeb(tool: Pick<ToolDefinition, "platforms">): boolean {
  return tool.platforms.includes("web");
}
