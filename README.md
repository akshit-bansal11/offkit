# Offkit

Offkit is 37 everyday file and developer tools for audio, video, images, PDFs, SVG, colours, CSS, text and dev work, and every one runs on your own machine. It is a native desktop app for Windows, macOS and Linux, and a web app that does the same work in the browser.

[Website](https://offkit.akshitbansal.me) · [Documentation](https://offkit.akshitbansal.me/docs) · [Releases](https://github.com/akshit-bansal11/offkit/releases/latest)

[![CI](https://github.com/akshit-bansal11/offkit/actions/workflows/ci.yml/badge.svg)](https://github.com/akshit-bansal11/offkit/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/akshit-bansal11/offkit)](https://github.com/akshit-bansal11/offkit/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-offkit.akshitbansal.me-black.svg)](https://offkit.akshitbansal.me/docs)

Offkit was called Open Tools before v0.2.0.

## What it does

- **Converts, edits and joins media.** Audio and video convert between formats and codecs, extract audio or frames, trim, change speed and pitch, apply EQ, and merge files.
- **Works on documents and images.** PDFs merge, split, compress and convert to and from images and other formats. Images convert and crop; palettes come out of a picture.
- **Makes design assets.** Gradients, palettes, CSS blobs, glass effects, box shadows and animated or patterned SVGs, with SVG optimisation.
- **Handles text and dev chores.** JSON, Base64, diffs, Markdown, lorem ipsum, archives (ZIP, TAR, TAR.GZ, GZ, 7z) and directory trees.
- **Stays on your device.** Files are processed locally. There is no upload and no account.
- **Runs in two places.** The desktop app and the website offer the same tools. A tool can be limited to one side; `tools.json` lists each tool's `platforms`, and today all 37 list both.

## Install

Download from the [latest release](https://github.com/akshit-bansal11/offkit/releases/latest), under **Assets**. Each file is named for its release, so `offkit-0.2.0.exe` belongs to 0.2.0. The release also carries `SHA256SUMS.txt`, and `gh attestation verify <file> --repo akshit-bansal11/offkit` checks that a file was built by this repository's release workflow. A `THIRD-PARTY-NOTICES.html` ships in each package.

### Windows

1. Download `offkit-<version>.exe`, a per-user installer.
2. Run it. It is not code-signed, so SmartScreen asks first: choose **More info**, then **Run anyway**.

### macOS

1. Download `offkit-<version>-arm64.dmg` for Apple Silicon or `offkit-<version>-x64.dmg` for Intel, and open it.
2. Move Offkit to Applications. It is ad hoc signed and not notarised, so macOS refuses it at first. Right-click the app and choose **Open**, or choose **Open Anyway** in **System Settings > Privacy & Security**.

### Linux

Download `offkit-<version>-x86_64.AppImage` (make it executable and run it) or `offkit-<version>-amd64.deb` (install it with your package manager, for example `sudo apt install ./offkit-<version>-amd64.deb`).

### FFmpeg for audio and video

FFmpeg is not bundled. The audio and video tools need `ffmpeg` and `ffprobe` installed, both in the same folder, either on your `PATH` or next to the Offkit executable:

- Windows: `winget install Gyan.FFmpeg`
- macOS: `brew install ffmpeg`
- Linux: `apt install ffmpeg`

Without them, those tools show an "FFmpeg wasn't found" screen with a **Get FFmpeg** button. Restart Offkit after installing. The other tools do not need FFmpeg. PDFium, which the PDF tools use, is bundled. The web app needs no install at all.

The desktop app does not update itself; install a newer release to upgrade.

## Use

1. Pick a tool in the sidebar, or search for it by name.
2. Drop files on the window, or use the tool's open button.
3. Set the options and run. Results are written as new files beside the source or in a folder you choose; an existing file is never overwritten without you picking it in a save dialog.

A tool that runs on the web only shows an **Open in Browser** button in the desktop app.

## Tools

- **Audio:** Audio Converter, Audio Extractor, Audio Editor, Audio Joiner.
- **Video:** Video Converter, Frames Extractor, Video Editor, Video Merger.
- **Dev:** Directory Scaffold, Archive Extractor, Archiver.
- **Images:** Image Cropper, Image Converter.
- **SVG:** SVG Border Animator, SVG Pattern Generator, SVG Optimizer.
- **Colors:** Gradient Maker, Gradient Library, Palette Library, Palette Extractor, Gradient Converter, Color Converter.
- **CSS:** CSS Blob Generator, Glassmorphism Generator, Box Shadow Generator.
- **PDF:** PDF Toolkit, PDF ↔ Image Converter, PDF Merger, PDF Splitter, PDF Compressor, Convert to PDF, Convert from PDF.
- **Text:** Lorem Generator, Diff Checker, JSON Formatter, Base64, Markdown Editor.

The full reference is on the [docs page](https://offkit.akshitbansal.me/docs), which is generated from `src/config/tools.ts`.

### Planned

| Tool | Category | Description |
|------|----------|-------------|
| PDF Background Remover & Changer | DOCUMENTS | Remove or swap backgrounds from PDFs |
| OCR | DOCUMENTS | Extract text from images and PDFs using optical character recognition |
| Subtitle Generator & Burner | VIDEO | Generate subtitles and burn them into video, powered by Trupeer |

## Privacy

- Files are processed on the device, in both apps.
- The desktop app has no analytics, no error reporting, no account and no upload code. It opens links in your default browser only on request: "Open in Browser" for a web-only tool, and the FFmpeg and PDFium download pages.
- On the website, Directory Scaffold reads a repository's file tree from `api.github.com` when you use it. The website also runs Vercel Web Analytics and Better Stack error reporting; the Directory Scaffold page loads no foreign script.
- The web app is served with a strict Content Security Policy; see [SECURITY.md](SECURITY.md).

## How it works

`tools.json` lists the tools once. The desktop app (`desktop/`, Rust and Slint) compiles it in and runs each tool on shared engines: FFmpeg for audio and video, PDFium and lopdf for PDFs, resvg for SVG, and pure-Rust crates for images, text and archives. The web app (`src/`, Next.js) runs FFmpeg and ImageMagick as WebAssembly in the browser. Untrusted input fails closed in both. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) has the layout, the engines and the security gates.

## Develop

The code is MIT-licensed, so you can build it, change it and run your own copy. To send a change back, fork the repository, clone your fork, and see [CONTRIBUTING.md](CONTRIBUTING.md).

| Path | What |
| --- | --- |
| `desktop/` | The native app. Rust and Slint. |
| `src/` | The web app. Next.js 16, React 19, TypeScript, Tailwind. |
| `tools.json` | The shared tool registry. |
| `e2e/` | Playwright specs for the web app. |
| `docs/` | Architecture. |

### Prerequisites

Either app can be worked on alone. The web app needs Node 24 (what CI uses). The desktop app needs a current stable Rust toolchain with `rustfmt` and `clippy`, plus `ffmpeg` and `ffprobe` on `PATH` to run or test the media tools; on Linux it also needs `libasound2-dev`.

### Quality gate

CI is the gate for both apps; the commands below are what it runs.

```bash
# web app, from the repository root
npm install
npm run check            # biome, eslint, stylelint, tsc; writes fixes
npm run check:ci         # the same without writing; what CI runs
# desktop app, from desktop/
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

CI also formats every tracked `.rs` file directly, because `cargo fmt` skips modules declared inside the `tools!` macro.

### Run locally

```bash
npm run dev                          # the website at http://localhost:3000
sh desktop/scripts/fetch-pdfium.sh   # once: PDFium for the desktop app
cd desktop && cargo run              # the desktop app
```

### Tests

```bash
npm test                    # web unit tests (node --test); includes the tools.json versus src/config/tools.ts check
npx playwright install chromium     # once
npm run test:e2e:web        # browser tests against a production build
cd desktop && cargo test --locked   # engines, registry and tool tests; media tests skip without ffmpeg
```

## Releasing

Set the same new version in `desktop/Cargo.toml` and `package.json`, commit, then push a tag `vX.Y.Z`. `.github/workflows/release.yml` builds the installers, attaches `SHA256SUMS.txt` and a build provenance attestation, and publishes a GitHub Release. The tag must match both versions. A manual run of the workflow makes a private draft build instead of a release.

## Contributing

- **A bug, or something Offkit should do:** [open an issue](https://github.com/akshit-bansal11/offkit/issues/new/choose). The templates ask for what is needed to act on it. Open one before starting anything bigger than a fix.
- **A security hole:** report it privately, as [SECURITY.md](SECURITY.md) describes, not in a public issue.
- **A change:** branch on your fork, run the checks until they are clean, and open a pull request against `main`. [CONTRIBUTING.md](CONTRIBUTING.md) has the details.

Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html), and `CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Support

Offkit is free and stays free. If it is useful to you and you would like to say so with money, there are three ways:

- **[Ko-fi](https://ko-fi.com/akshit_bansal11)**, from anywhere.
- **[PayPal](https://paypal.me/AkshitBansal141)**, from anywhere.
- **UPI**, from any UPI app in India: `artistbansal2004@okaxis`

A star, a bug report or telling someone about it helps as much.

## License

[MIT](LICENSE).

## Credits

- Icons are [Lucide](https://lucide.dev), ISC licence; some came from Feather (MIT). The licence text is in `desktop/ui/icons/LICENSE`.
- The interface typeface is Inter and code is set in JetBrains Mono, both under the SIL Open Font License; the licences are in `desktop/ui/fonts/`.
- PDF tools in the desktop app use [PDFium](https://pdfium.googlesource.com/pdfium/) (BSD-3-Clause and Apache-2.0), from [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries). FFmpeg is a separate install and is not distributed with Offkit.
- The web app runs FFmpeg (`@ffmpeg/core`, GPL-2.0-or-later) and ImageMagick (`@imagemagick/magick-wasm`) as WebAssembly.
- Every bundled third-party component's licence is in the `THIRD-PARTY-NOTICES.html` that ships in each package.
- Built by [Akshit Bansal](https://github.com/akshit-bansal11) ([LinkedIn](https://linkedin.com/in/akshit-bansal11)).
