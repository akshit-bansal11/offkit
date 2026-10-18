# Changelog

All notable changes to Offkit, formerly Open Tools. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The desktop app and the website share one version number.

## [Unreleased]

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

[Unreleased]: https://github.com/akshit-bansal11/offkit/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/akshit-bansal11/offkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/akshit-bansal11/offkit/releases/tag/v0.1.0
