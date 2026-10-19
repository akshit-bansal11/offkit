# Changelog

All notable changes to Offkit, formerly Open Tools. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The desktop app and the website share one version number.

## [Unreleased]

## [0.2.1] - 2026-10-20

A sweep of interface bugs in both apps, plus interaction polish.

### Fixed

- **Website: dead sliders and buttons.** Every colour class with an opacity (`bg-primary/20`, `hover:bg-primary/90`, `ring-ring/50` and 231 more) produced no CSS, so slider tracks were invisible and buttons and fields showed no hover or focus. Sliders now draw a filled track in the tool's accent.
- **Website: controls that could not be reached.** The Video Editor's trim end could not be dragged; Blob Generator's marks, and decorative layers in Glassmorphism, Palette Library, PDF Splitter and PDF Converter, sat over controls and took their clicks.
- **Website: keyboard access.** Two checkboxes built from a `div` in Video Editor and Video Merger are real switches, hover-only remove buttons appear on keyboard focus, and icon-only buttons have names.
- **Website: colours.** Status messages use the Offkit palette (coral, mint, citrus, sky) instead of stock reds, greens, ambers and blues, with measured contrast.
- **Desktop: slider drags.** Blob Generator's sliders stopped after the first movement, and Box Shadow's layer list lost keyboard focus on each change: both rebuilt their rows on every edit. Sliders now also snap to their step.
- **Desktop: overlapping text.** A wrapped note under an inspector section was measured as one line, so the next section drew over it, in seven tools.
- **Desktop: layout.** PDF Compressor's inspector overflowed sideways, Image Cropper elided a label to "C…", and Gradient Maker showed a sideways scrollbar.
- **Desktop: the dark switch knob** was nearly invisible on the citrus track.

### Changed

- Both apps give sliders, buttons, switches, checkboxes and list rows hover and press feedback, and respect reduced motion. Desktop sliders take Home, End, Page Up and Page Down.
- PDF Compressor's methods are labelled Recompress and Rasterise, and Image Cropper's scope Each and All.

## [0.2.0] - 2026-10-18

Open Tools is now **Offkit**. The Electron app from 0.1.0 is gone: the desktop app is rewritten as a native Rust and Slint program, and the website serves every tool in the browser again. Both read one tool registry, so a tool that only works on one side says so on the other.

### Added

- **A native desktop app for Windows, macOS and Linux**, written in Rust with a Slint interface. It has 37 tools in nine families:
  - **Audio:** Audio Converter, Audio Editor (waveform, trim, speed, pitch, EQ and an in-app preview), Audio Extractor, Audio Joiner.
  - **Video:** Video Converter, Video Editor (crop, rotate, flip, scale, speed, trim), Video Merger, Frames Extractor.
  - **Images:** Image Converter, Image Cropper.
  - **PDF** (on a bundled PDFium): PDF Toolkit, PDF ↔ Image Converter, PDF Merger, PDF Splitter, PDF Compressor, Convert to PDF, Convert from PDF.
  - **SVG:** SVG Optimizer, SVG Pattern Generator, SVG Border Animator.
  - **Colours:** Color Converter, Gradient Maker, Gradient Library, Gradient Converter, Palette Library, Palette Extractor.
  - **CSS:** CSS Blob Generator, Box Shadow Generator, Glassmorphism Generator.
  - **Text:** Diff Checker, JSON Formatter, Lorem Generator, Markdown Editor, Base64.
  - **Dev:** Archiver (ZIP, TAR.GZ, 7z), Archive Extractor, Directory Scaffold.
- **Installers on GitHub Releases:** `offkit-<version>.exe` for Windows, `offkit-<version>-arm64.dmg` and `offkit-<version>-x64.dmg` for macOS, and `offkit-<version>-x86_64.AppImage` and `offkit-<version>-amd64.deb` for Linux. Each release also carries `SHA256SUMS.txt` and a build provenance attestation.
  - The Windows installer is unsigned, and the macOS app is ad hoc signed but not notarised, so both systems warn on first launch.
  - FFmpeg is not bundled. The audio and video tools ask you to install it, while PDFium ships inside every package.
- **Per-tool platforms.** `tools.json` records where each tool runs. The desktop app opens web-only tools in the browser, and the website points desktop-only tools to the app.
- **The app icon:** an off switch on a citrus tile.

### Changed

- **Renamed to Offkit.** The repository, the website and the app now carry the new name.
- **The website serves every tool again,** with security headers, instead of the landing and docs pages it was cut down to in 0.1.0. Seven tool pages reflow at a 320px width.
- **Git Scaffold is now Directory Scaffold.** On the desktop it draws the tree of a local folder and honours `.gitignore`.
- **Both apps use the Offkit palette:** ink, citrus, mint, coral, sky and paper. Every text and control colour was measured against WCAG 2.2 AA in both light and dark.

### Removed

- The Electron desktop app.

### Security

- Media is opened only through plain-file demuxers, and those are checked before FFmpeg opens the file. Playlists and non-file protocols are refused, and ffprobe's duration, time and output are bounded.
- The Archive Extractor fails closed. It guards 7z encoded headers, counts TAR extension headers and bounds archive depth. Directory Scaffold caps how much of a folder it scans and matches.
- SVG input goes through a locked-down renderer. The Border Animator's output is sanitised with a case-insensitive allowlist and keeps `url()` references local.
- PDF render sizes and thumbnails are capped.

## [0.1.0] - 2026-10-13

The first tagged release, as Open Tools.

### Added

- An Electron desktop app holding the tools. The website was reduced to a landing page and docs.
- The WebAssembly engines and the pdf.js worker ship with the app, so the tools work offline.
- On-device colour extraction for the Palette Extractor, replacing a call to Gemini.
- A quality gate, a GitHub Actions workflow, Vercel Web Analytics and Better Stack error reporting.

### Fixed

- The FFmpeg tools no longer fail silently or offer codecs that cannot work.
- The Image Cropper was repaired and enabled alongside the PDF toolkit.
- The gradient type switch now works, and PDF errors show inline.
- The Video Merger normalises clips by default.

[Unreleased]: https://github.com/akshit-bansal11/offkit/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/akshit-bansal11/offkit/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/akshit-bansal11/offkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/akshit-bansal11/offkit/releases/tag/v0.1.0
