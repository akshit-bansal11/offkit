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

| Tool | Category | Description |
|------|----------|-------------|
| Audio Converter | AUDIO | Convert audio files between formats and codecs locally using ffmpeg.wasm |
| Audio Extractor | AUDIO | Extract the audio track from any video file in your chosen format and codec |
| Git Scaffold | DEV | Paste a public GitHub repo URL and instantly visualize its full directory structure |
| SVG Border Animator | DESIGN | Upload any SVG and animate each path border with stroke-dashoffset controls |
| SVG Pattern Generator | DESIGN | Create beautiful, scalable vector patterns — export as CSS backgrounds or SVG data URIs |
| CSS Blob Generator | DESIGN | Create intricate organic shapes by independently customizing 8 anchor points |
| Glassmorphism Generator | DESIGN | Design frosted-glass UI elements with real-time backdrop filtering |
| Box Shadow Generator | DESIGN | Create layered CSS shadows natively and export the exact styling string |
| PDF Toolkit | DOCUMENTS | Merge, split by page, reorder, or compress PDFs directly in the browser |
| Lorem Generator | TEXT | Generate customized placeholder text using classic Latin or random English prose |
| Diff Checker | TEXT | Find inline character-level or line-level text differences with robust ignoring options |
| SVG Optimizer | DESIGN | Clean up messy SVGs, strip metadata, dial in decimal precision, and convert to JSX |
| PDF ↔ Image | DOCUMENTS | Merge images into a PDF, batch convert them, or extract pages as images |
| Image Converter | IMAGES | Convert popular image formats locally with batch downloads and quality controls |
| JSON Formatter | TEXT | Pretty-print, minify, validate, and copy JSON without leaving the browser |
| Gradient Maker | DESIGN | Build multi-stop gradients with live previews, PNG exports, and Tailwind arbitrary values |
| Gradient Library | DESIGN | Browse curated named gradients and copy them as CSS or Tailwind-ready arbitrary values |
| Palette Library | DESIGN | Explore curated color palettes, copy swatches, or export full palettes as JSON |
| Palette Extractor | DESIGN | Upload an image and extract its dominant colors into a copyable palette, computed locally in the browser |
| Gradient Converter | DESIGN | Convert CSS gradients into Tailwind arbitrary values or switch between linear/radial/conic |
| Color Converter | DESIGN | Convert HEX, RGB, HSL, HSV, OKLCH, and named colors into every other format at once |
| Base64 | ENCODING | Encode and decode text with a clean local workflow that stays entirely in the browser |

---

### 🚧 In Development

| Tool | Category | Description |
|------|----------|-------------|
| Video Converter | VIDEO | Convert videos between formats and codecs directly in the browser using ffmpeg.wasm |
| Frame Extractor | VIDEO | Upload a GIF or video and extract every frame as individual PNG images, then download as a ZIP |
| Image Cropper | IMAGES | Crop and apply individual crops per image or a single crop to all at once in the browser |

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

Tool-based architecture: each utility has its own route in `src/app/(tools)/<tool-name>/` with a local `_components` folder.

```
src/
├── app/
│   ├── globals.css
│   ├── layout.tsx
│   ├── page.tsx
│   ├── (tools)/
│   │   ├── audio-converter/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── AudioConverterTool.tsx
│   │   ├── audio-extractor/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── AudioExtractorTool.tsx
│   │   ├── base64/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── Base64Tool.tsx
│   │   ├── blob-generator/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── BlobGeneratorTool.tsx
│   │   ├── box-shadow/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── BoxShadowTool.tsx
│   │   ├── color-converter/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── ColorConverterTool.tsx
│   │   ├── diff-checker/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── DiffCheckerTool.tsx
│   │   ├── frames-extractor/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── FramesExtractorTool.tsx
│   │   ├── git-scaffold/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── GitScaffoldTool.tsx
│   │   ├── glassmorphism/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── GlassmorphismTool.tsx
│   │   ├── gradient-converter/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── GradientConverterTool.tsx
│   │   ├── gradient-library/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── GradientLibraryTool.tsx
│   │   ├── gradient-maker/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── GradientMakerTool.tsx
│   │   ├── image-converter/
│   │   │   ├── layout.tsx
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── ImageConverter.tsx
│   │   ├── image-cropper/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── ImageCropperTool.tsx
│   │   ├── json-formatter/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── JsonFormatter.tsx
│   │   ├── lorem-generator/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── LoremGeneratorTool.tsx
│   │   ├── palette-extractor/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── PaletteExtractorTool.tsx
│   │   ├── palette-library/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── PaletteLibraryTool.tsx
│   │   ├── pdf-converter/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── PdfConverterTool.tsx
│   │   ├── pdf-toolkit/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── PdfToolkitTool.tsx
│   │   ├── svg-animator/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── SvgAnimatorTool.tsx
│   │   ├── svg-optimizer/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── SvgOptimizerTool.tsx
│   │   ├── svg-pattern/
│   │   │   ├── page.tsx
│   │   │   └── _components/
│   │   │       └── SvgPatternTool.tsx
│   │   └── video-converter/
│   │       ├── page.tsx
│   │       └── _components/
│   │           └── VideoConverterTool.tsx
│   └── api/
│       └── magick-wasm/
│           └── route.ts
├── components/
│   ├── repository-corner.tsx
│   ├── site-footer.tsx
│   ├── tool-page-shell.tsx
│   ├── design-tools/
│   │   ├── copy-button.tsx
│   │   ├── field.tsx
│   │   ├── gemini-api-key-dialog.tsx
│   │   └── output-field.tsx
│   └── ui/
│       ├── badge.tsx
│       ├── button.tsx
│       ├── card.tsx
│       ├── input.tsx
│       ├── label.tsx
│       ├── progress.tsx
│       ├── select.tsx
│       ├── separator.tsx
│       ├── slider.tsx
│       ├── textarea.tsx
│       └── tooltip.tsx
├── config/
│   ├── site.ts
│   └── tools.ts
├── lib/
│   ├── utils.ts
│   ├── converters/
│   │   └── image-converter/
│   │       ├── converter.ts
│   │       └── engine.ts
│   ├── design-tools/
│   │   ├── colors.ts
│   │   ├── constants.ts
│   │   ├── gradients.ts
│   │   ├── palette-extractor.ts
│   │   └── data/
│   │       ├── gradient-presets.ts
│   │       └── palette-presets.ts
│   └── ffmpeg/
│       └── client.ts
└── types/
    └── tool.ts

```

The homepage is fully driven by `config/tools.ts`. New tools auto-appear on the landing page once registered — no scattered metadata.

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

# 5) Run lint checks
npm run lint
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
