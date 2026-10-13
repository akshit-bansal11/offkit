# open-tools

[![Next.js](https://img.shields.io/badge/Next.js-16-black)](https://nextjs.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178c6)](https://www.typescriptlang.org)
[![React](https://img.shields.io/badge/React-19-149eca)](https://react.dev)
[![Local-first](https://img.shields.io/badge/Platform-Local--first-22c55e)](https://github.com/akshit-bansal11/open-tools)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Project Name + Description

**open-tools** is a growing collection of fast, local-first browser utilities for developers and designers — no logins, no uploads to servers, no fluff.

Each tool lives at its own route under `app/(tools)/`. Adding a new tool is as simple as dropping a route and registering it in the central tool registry.

---

## Features

### ✅ Currently Available

All 37 tools run entirely in the browser. Files are never uploaded.

| Tool | Category | Description |
|------|----------|-------------|
| Audio Converter | Audio | Convert audio files between formats and codecs entirely in the browser using ffmpeg.wasm. |
| Audio Editor | Audio | Trim, adjust speed, shift pitch, and apply EQ to any audio file — all in the browser. |
| Audio Extractor | Audio | Extract the audio track from any video file and download it in your chosen format and codec. |
| Audio Joiner | Audio | Drag-and-drop multiple audio files and join them into a single file in your chosen format. |
| Frames Extractor | Video | Upload a GIF or video and extract every frame as individual PNG images, then download them as a ZIP. |
| Video Converter | Video | Convert videos between formats and codecs directly in the browser using ffmpeg.wasm. |
| Video Editor | Video | Trim, crop, rotate, resize, flip, adjust speed and volume for any video — entirely in the browser. |
| Video Merger | Video | Drag-and-drop multiple video files, reorder them, and merge into a single video. |
| Image Converter | Images | Convert popular image formats locally with batch downloads, quality controls, and upload guardrails. |
| Image Cropper | Images | Crop one or many images in the browser - apply individual crops per image or a single crop to all at once. |
| Convert from PDF | PDF | Extract PDF pages as PNG or JPEG images and download them as a ZIP. |
| Convert to PDF | PDF | Convert images to PDF — drop multiple images and combine them into a single document. |
| PDF Compressor | PDF | Reduce PDF file size by rasterizing pages at adjustable quality and resolution. |
| PDF Merger | PDF | Combine multiple PDF documents into one file with drag-to-reorder. |
| PDF Splitter | PDF | Break a PDF into individual pages or extract specific page ranges. |
| PDF Toolkit | PDF | Merge multiple files, break PDFs apart by page, reorder structure, or compress files directly in your browser. |
| PDF ↔ Image Converter | PDF | Merge images into a single PDF, batch convert them, or extract pages from existing PDFs as images. |
| SVG Border Animator | SVG | Upload any SVG and animate each path border locally with stroke-dashoffset controls and export-ready output. |
| SVG Optimizer | SVG | Clean up messy SVGs, strip metadata, dial in decimal precision, and convert to JSX directly in the browser. |
| SVG Pattern Generator | SVG | Create beautiful, scalable vector patterns directly in your browser. Export native CSS backgrounds or raw SVG data URIs instantly. |
| Color Converter | Colors | Convert HEX, RGB, HSL, HSV, OKLCH, and named colors into every other format at once. |
| Gradient Converter | Colors | Convert CSS gradients into Tailwind arbitrary values or switch between linear, radial, and conic syntax. |
| Gradient Library | Colors | Browse curated named gradients and copy them as CSS or Tailwind-ready arbitrary values. |
| Gradient Maker | Colors | Build multi-stop gradients with live previews, PNG exports, CSS output, and Tailwind arbitrary values. |
| Palette Extractor | Colors | Upload an image and extract its dominant colors into a copyable palette, computed locally in your browser. |
| Palette Library | Colors | Explore curated color palettes, copy individual swatches, or export full palettes as JSON. |
| Box Shadow Generator | CSS | Create layered CSS shadows natively through visual controls and export the exact styling string for your projects. |
| CSS Blob Generator | CSS | Create intricate, organic shapes natively by independently customizing the 8 anchor points of the border-radius property. |
| Glassmorphism Generator | CSS | Design stunning frosted-glass UI elements with real-time backdrop filtering and export the CSS directly. |
| Base64 | Text | Encode and decode text with a clean local workflow that stays entirely in the browser. |
| Diff Checker | Text | Find inline character-level or line-level text differences with robust ignoring whitespace and case sensitivity. |
| JSON Formatter | Text | Pretty-print, minify, validate, and copy JSON without leaving the browser. |
| Lorem Generator | Text | Generate customized placeholder text instantly for your mockups, using either classic Latin or random English prose. |
| Markdown Editor | Text | Write Markdown with a live split-pane preview, toolbar shortcuts, and export to .md or PDF. |
| Archive Extractor | Dev | Upload a ZIP archive, preview its file tree, and extract individual files or download everything. |
| Archiver | Dev | Drop files to create a ZIP archive instantly in the browser. |
| Git Scaffold | Dev | Paste a public GitHub repository URL and instantly visualize its full directory structure including all filenames. |

---

### 🗓 Planned

| Tool | Category | Description |
|------|----------|-------------|
| PDF Background Remover & Changer | DOCUMENTS | Remove or swap backgrounds from PDFs directly in the browser |
| OCR | DOCUMENTS | Extract text from images and PDFs using optical character recognition |
| Subtitle Generator & Burner | VIDEO | Generate subtitles and burn them into video — powered by Trupeer |

---

## Tech Stack

- **Framework/App Runtime:** Next.js 16, React 19
- **Language:** TypeScript
- **Styling/UI:** Tailwind CSS, shadcn/ui (Radix UI primitives + `class-variance-authority` + `tailwind-merge`)
- **Media processing:** FFmpeg.wasm (`@ffmpeg/ffmpeg`, `@ffmpeg/util`), ImageMagick (`@imagemagick/magick-wasm`)
- **Document/Image tooling:** `pdf-lib`, `pdfjs-dist`, `jszip`, `svgo`
- **Text/utility tooling:** `diff`, `marked`, `dompurify`

---

## Directory Structure

Each tool has its own route under `src/app/(tools)/<slug>/`, made of a `layout.tsx` (metadata) and a `page.tsx` (the tool).

```
src/
├── app/
│   ├── (tools)/<slug>/        one folder per tool: layout.tsx + page.tsx
│   ├── layout.tsx             root layout
│   └── page.tsx               home page, generated from the registry
├── components/
│   ├── common/                page shell, category cards, tool cards
│   ├── tools/<slug>/          components specific to one tool
│   └── ui/                    shared primitives (form, feedback, layout, interaction)
├── config/tools.ts            the tool registry: every tool and category
├── data/inDevelopmentSlugs.ts slugs hidden from the home page
├── lib/
│   ├── ffmpeg/client.ts       ffmpeg.wasm loader, checked runner, codec tables
│   ├── converters/            ImageMagick conversion engine
│   ├── design-tools/          colour, gradient and palette logic
│   └── tools/<slug>/          logic specific to one tool
└── types/tool.ts
```

The homepage is fully driven by `config/tools.ts`. A tool appears on the landing page once it is registered there.

---

## Running Steps

```bash
# 1) Install dependencies
npm install

# 2) Start development server
npm run dev

# 3) Build for production
npm run build

# 4) Start production server
npm run start

# 5) Lint and type-check (writes fixes)
npm run check

# 6) Unit tests
npm test
```

Open [http://localhost:3000](http://localhost:3000).

---

## Contributing
 
Contributions are welcome — PRs and issues both.
 
**To add a tool:**
 
1. Create a route under `src/app/(tools)/your-tool/`
2. Register it in `src/config/tools.ts`
3. Use `ToolPageShell` as the layout wrapper for consistency
 
Keep tools self-contained and client-side where possible.
 
**To report a bug or request a feature:**
 
Open an issue with a clear title and description. For bugs, include steps to reproduce and your browser/OS. For feature requests, describe the use case — not just the solution.
 
---

## Links

- **Live:** [use-open-tools.vercel.app](https://use-open-tools.vercel.app)
- **GitHub:** [github.com/akshit-bansal11](https://github.com/akshit-bansal11)
- **LinkedIn:** [linkedin.com/in/akshit-bansal11](https://linkedin.com/in/akshit-bansal11)
