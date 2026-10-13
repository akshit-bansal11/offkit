# Open Tools

[![Next.js](https://img.shields.io/badge/Next.js-16-black)](https://nextjs.org)
[![Electron](https://img.shields.io/badge/Electron-44-47848f)](https://www.electronjs.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178c6)](https://www.typescriptlang.org)
[![React](https://img.shields.io/badge/React-19-149eca)](https://react.dev)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows-22c55e)](https://github.com/akshit-bansal11/open-tools/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Open Tools is a Windows desktop app with 37 file tools for images, video, audio, PDFs, SVG, colors and text. Everything runs on your PC. Nothing is uploaded. There is no account and no telemetry.

## Install

Download `Open-Tools-Setup.exe` from the [latest release](https://github.com/akshit-bansal11/open-tools/releases/latest), or `Open-Tools-Portable.exe` to run without installing.

- The installer is not code-signed, so Windows SmartScreen may warn. Choose More info, then Run anyway.
- The app does not update itself.

Documentation: [open-tools.akshitbansal.me/docs](https://open-tools.akshitbansal.me/docs)

## Tools

- **Audio:** Audio Converter, Audio Extractor, Audio Editor, Audio Joiner.
- **Video:** Video Converter, Frames Extractor, Video Editor, Video Merger.
- **Dev:** Git Scaffold, Archive Extractor, Archiver.
- **Images:** Image Cropper, Image Converter.
- **SVG:** SVG Border Animator, SVG Pattern Generator, SVG Optimizer.
- **Colors:** Gradient Maker, Gradient Library, Palette Library, Palette Extractor, Gradient Converter, Color Converter.
- **CSS:** CSS Blob Generator, Glassmorphism Generator, Box Shadow Generator.
- **PDF:** PDF Toolkit, PDF ↔ Image Converter, PDF Merger, PDF Splitter, PDF Compressor, Convert to PDF, Convert from PDF.
- **Text:** Lorem Generator, Diff Checker, JSON Formatter, Base64, Markdown Editor.

The full reference is on the docs page, which is generated from `src/config/tools.ts`.

### Planned

| Tool | Category | Description |
|------|----------|-------------|
| PDF Background Remover & Changer | DOCUMENTS | Remove or swap backgrounds from PDFs |
| OCR | DOCUMENTS | Extract text from images and PDFs using optical character recognition |
| Subtitle Generator & Burner | VIDEO | Generate subtitles and burn them into video, powered by Trupeer |

## Privacy

- Files are processed on the device.
- The app cancels requests a page makes to the network. The one exception is a read of a repository's file tree from `api.github.com`, which Git Scaffold makes when you use it.
- Permissions are refused, except clipboard write, and clipboard read on the Diff Checker.
- Links open in your default browser only when they point at this project's GitHub repository.
- The app has no analytics or error reporting. Those exist only on the website.

## How it is built

One Next.js codebase, two build targets chosen by `BUILD_TARGET`.

| Target | Routes | Output | Ships as |
|--------|--------|--------|----------|
| desktop | `page.desktop.tsx` files: launcher and tools | static export in `out/` | Electron app, NSIS installer and portable exe on GitHub Releases |
| web | `page.web.tsx` files: landing page and docs | normal Next build | the website |

The wasm engines and the pdf.js worker are copied from `node_modules` into `public/vendor/` by `scripts/copy-vendor.mjs`, so the app never fetches them from a CDN.

- **App shell:** Electron, electron-builder
- **Framework:** Next.js 16, React 19
- **Language:** TypeScript
- **Styling/UI:** Tailwind CSS, shadcn/ui (Radix UI primitives + `class-variance-authority` + `tailwind-merge`)
- **Media processing:** FFmpeg (WebAssembly build, `@ffmpeg/ffmpeg` + `@ffmpeg/core`), ImageMagick (`@imagemagick/magick-wasm`)
- **Document/Image tooling:** `pdf-lib`, `pdfjs-dist`, `jszip`, `svgo`
- **Text/utility tooling:** `diff`, `marked`, `dompurify`

## Directory structure

```
src/
├── app/
│   ├── (tools)/<slug>/        one folder per tool: layout.tsx + page.desktop.tsx
│   ├── docs/page.web.tsx      docs page (website)
│   ├── layout.tsx             root layout
│   ├── page.desktop.tsx       launcher (app)
│   ├── page.web.tsx           landing page (website)
│   ├── not-found.web.tsx
│   ├── opengraph-image.tsx
│   ├── robots.ts
│   └── sitemap.ts
├── components/
│   ├── common/                page shell, category cards, tool cards
│   ├── site/                  website shell and observability
│   ├── tools/<slug>/          components specific to one tool
│   └── ui/                    shared primitives
├── config/
│   ├── site.ts                website constants
│   ├── target.ts              build target
│   └── tools.ts               the tool registry: every tool and category
├── data/inDevelopmentSlugs.ts slugs hidden from the launcher
├── lib/
│   ├── ffmpeg/client.ts       FFmpeg loader, checked runner, codec tables
│   ├── converters/            ImageMagick conversion engine
│   ├── design-tools/          colour, gradient and palette logic
│   ├── pdf/pdfjs.ts           pdf.js loader
│   └── tools/<slug>/          logic specific to one tool
└── types/tool.ts
electron/main.mjs              Electron main process
electron-builder.yml           installer config
scripts/                       copy-vendor, next-desktop
e2e/                           Playwright specs
```

## Development

```bash
npm install                 # install dependencies
npm run dev:desktop         # the tools, in a browser at http://localhost:3000
npm run dev                 # the website
npm run build:desktop       # build the static export into out/
npm run app                 # run the desktop app from the export
npm run dist                # build the installers into release/
npm run check               # format, lint, typecheck (writes fixes)
npm test                    # unit tests
npm run test:e2e:desktop    # browser tests against the desktop build
npm run test:e2e:web        # browser tests against the website
```

## Releasing

Bump `version` in `package.json`, commit, then push a tag `v<version>`. `.github/workflows/release.yml` builds on Windows, runs the unit tests and the tool tests against the packaged app, attaches build provenance and publishes both exe files to a GitHub Release. The tag must match the `package.json` version or the workflow stops.

## Contributing

Contributions are welcome, as PRs and issues.

**To add a tool:**

1. Create `src/app/(tools)/your-tool/` with `layout.tsx` and `page.desktop.tsx`.
2. Register it in `src/config/tools.ts`. The launcher and the docs page both pick it up from there.
3. Use `ToolPageShell` as the layout wrapper.

Tools must work offline: no CDN, no network requests.

**To report a bug or request a feature:** open an issue with a clear title and description. For bugs, include steps to reproduce and your Windows version. For feature requests, describe the use case, not just the solution.

## Licence

The project's code is MIT, see [LICENSE](LICENSE). The packaged app bundles third-party engines under their own licences, including FFmpeg (`@ffmpeg/core`, GPL-2.0-or-later) and ImageMagick.

## Links

- **Website:** [open-tools.akshitbansal.me](https://open-tools.akshitbansal.me)
- **Docs:** [open-tools.akshitbansal.me/docs](https://open-tools.akshitbansal.me/docs)
- **GitHub:** [github.com/akshit-bansal11](https://github.com/akshit-bansal11)
- **LinkedIn:** [linkedin.com/in/akshit-bansal11](https://linkedin.com/in/akshit-bansal11)
