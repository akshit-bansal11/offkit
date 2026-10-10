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
import type { ToolCategoryDefinition, ToolDefinition } from "@/types/tool";

export const toolCategories: ToolCategoryDefinition[] = [
  { name: "Audio", icon: AudioLines, accentColor: "#60a5fa" },
  { name: "Video", icon: Video, accentColor: "#facc15" },
  { name: "Dev", icon: Code2, accentColor: "#4ade80" },
  { name: "Images", icon: ImageIcon, accentColor: "#f87171" },
  { name: "SVG", icon: VectorSquare, accentColor: "#f472b6" },
  { name: "Colors", icon: SwatchBook, accentColor: "#a78bfa" },
  { name: "CSS", icon: FileCode2, accentColor: "#fb923c" },
  { name: "PDF", icon: FileText, accentColor: "#34d399" },
  { name: "Text", icon: Type, accentColor: "#38bdf8" },
];

export const tools: ToolDefinition[] = [
  {
    slug: "audio-converter",
    href: "/audio-converter",
    name: "Audio Converter",
    shortName: "Convert",
    description:
      "Convert audio files between formats and codecs entirely in the browser using ffmpeg.wasm.",
    category: "Audio",
    highlights: [
      "Per-file format + codec controls",
      "Batch conversion with ZIP downloads",
    ],
    icon: ArrowLeftRight,
  },
  {
    slug: "audio-extractor",
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
    href: "/audio-editor",
    name: "Audio Editor",
    shortName: "Edit",
    description:
      "Trim, adjust speed, shift pitch, and apply EQ to any audio file — all in the browser.",
    category: "Audio",
    highlights: ["Waveform trim handles", "Speed, pitch & EQ controls"],
    icon: SlidersHorizontal,
  },
  {
    slug: "audio-joiner",
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
    href: "/video-converter",
    name: "Video Converter",
    shortName: "Convert",
    description:
      "Convert videos between formats and codecs directly in the browser using ffmpeg.wasm.",
    category: "Video",
    highlights: [
      "Per-file video and audio codec mapping",
      "Batch conversion and ZIP export",
    ],
    icon: ArrowLeftRight,
  },
  {
    slug: "frames-extractor",
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
    slug: "git-scaffold",
    href: "/git-scaffold",
    name: "Git Scaffold",
    shortName: "Scaffold",
    description:
      "Paste a public GitHub repository URL and instantly visualize its full directory structure including all filenames.",
    category: "Dev",
    highlights: ["Collapsible file tree UI", "ASCII tree copy and TXT export"],
    icon: FolderTree,
  },
  {
    slug: "image-cropper",
    href: "/image-cropper",
    name: "Image Cropper",
    shortName: "Crop",
    description:
      "Crop one or many images in the browser - apply individual crops per image or a single crop to all at once.",
    category: "Images",
    highlights: [
      "Individual and batch crop modes",
      "Canvas drag-resize crop handles",
    ],
    icon: Crop,
  },
  {
    slug: "svg-animator",
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
    href: "/svg-pattern",
    name: "SVG Pattern Generator",
    shortName: "Pattern",
    description:
      "Create beautiful, scalable vector patterns directly in your browser. Export native CSS backgrounds or raw SVG data URIs instantly.",
    category: "SVG",
    highlights: [
      "Injected vector properties directly generated",
      "CSS backgrounds",
    ],
    icon: Grid3X3,
  },
  {
    slug: "blob-generator",
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
    href: "/pdf-toolkit",
    name: "PDF Toolkit",
    shortName: "Toolkit",
    description:
      "Merge multiple files, break PDFs apart by page, reorder structure, or compress files directly in your browser.",
    category: "PDF",
    highlights: ["Compress documents securely natively"],
    icon: FileText,
  },
  {
    slug: "lorem-generator",
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
    href: "/svg-optimizer",
    name: "SVG Optimizer",
    shortName: "Optimize",
    description:
      "Clean up messy SVGs, strip metadata, dial in decimal precision, and convert to JSX directly in the browser.",
    category: "SVG",
    highlights: ["Live visual preview with JSX conversion"],
    icon: Sparkles,
  },
  {
    slug: "pdf-converter",
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
    href: "/json-formatter",
    name: "JSON Formatter",
    shortName: "Format",
    description:
      "Pretty-print, minify, validate, and copy JSON without leaving the browser.",
    category: "Text",
    highlights: ["Pretty print or minify in one click"],
    icon: FileJson,
  },
  {
    slug: "gradient-maker",
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
    href: "/palette-extractor",
    name: "Palette Extractor",
    shortName: "Extract",
    description:
      "Upload an image and extract its dominant colors into a copyable palette, computed locally in your browser.",
    category: "Colors",
    highlights: ["On-device dominant color extraction"],
    icon: ImageUp,
  },
  {
    slug: "gradient-converter",
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
    href: "/base64",
    name: "Base64",
    shortName: "Base64",
    description:
      "Encode and decode text with a clean local workflow that stays entirely in the browser.",
    category: "Text",
    highlights: ["Unicode-safe encode and decode"],
    icon: Binary,
  },

  {
    slug: "video-editor",
    href: "/video-editor",
    name: "Video Editor",
    shortName: "Edit",
    description:
      "Trim, crop, rotate, resize, flip, adjust speed and volume for any video — entirely in the browser.",
    category: "Video",
    highlights: ["Trim, crop, rotate & resize", "Speed & volume adjustment"],
    icon: Film,
  },
  {
    slug: "video-merger",
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
    href: "/convert-to-pdf",
    name: "Convert to PDF",
    shortName: "To PDF",
    description:
      "Convert images to PDF — drop multiple images and combine them into a single document.",
    category: "PDF",
    highlights: ["Drag-to-reorder images", "Fit, A4, or Letter page sizes"],
    icon: FileOutput,
  },
  {
    slug: "convert-from-pdf",
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
    href: "/archiver",
    name: "Archiver",
    shortName: "Archive",
    description: "Drop files to create a ZIP archive instantly in the browser.",
    category: "Dev",
    highlights: ["Editable file paths", "DEFLATE compression"],
    icon: FolderArchive,
  },
];

export function getToolBySlug(slug: string) {
  return tools.find((tool) => tool.slug === slug);
}

export const toolsByCategory = toolCategories.map((category) => ({
  ...category,
  tools: tools.filter((tool) => tool.category === category.name),
}));

export function getToolAccentColor(slug: string): string {
  const tool = getToolBySlug(slug);
  if (!tool) return "#60a5fa";
  const category = toolCategories.find((c) => c.name === tool.category);
  return category?.accentColor ?? "#60a5fa";
}
