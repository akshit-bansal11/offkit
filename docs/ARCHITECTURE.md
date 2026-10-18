# Architecture

Offkit is two apps in one repository, built from one list of tools.

| Path | What |
| --- | --- |
| `tools.json` | The shared tool registry: `siteUrl`, the categories, and every tool with its `platforms`. |
| `src/` | The web app. Next.js 16, React 19, TypeScript. Served at https://offkit.akshitbansal.me. |
| `desktop/` | The native app. Rust and Slint, for Windows, macOS and Linux. |
| `e2e/` | Playwright specs for the web app. |
| `scripts/` | `copy-vendor.mjs` copies the wasm engines into `public/vendor/`. |
| `.github/workflows/` | CI, the release workflow and the lockfile workflow. |

The apps share no code. They share `tools.json` and the same offline promise: files are processed on the device.

## tools.json

The file holds:

- `siteUrl`, the website's address. The desktop app builds `{siteUrl}/{slug}` from it, and only opens it when it starts with `https://`.
- `categories`, each with a `name` and an `accent` colour. The web app uses the accent; the desktop app ignores it and keeps one UI accent (`desktop/ui/theme.slint`).
- `tools`, each with `slug`, `name`, `shortName`, `description`, `category`, `icon` and `platforms`.

`platforms` is a list of `"web"` and `"desktop"`. A tool that omits `desktop` has no view in the desktop app; the shell shows a shared "This tool runs in your browser" panel with an Open in Browser button (`desktop/ui/tools/web-only.slint`). A tool that omits `web` keeps its URL on the site but shows a download panel (`src/components/site/PlatformGate.tsx`, `runsOnWeb` in `src/config/tools.ts`). Today all 37 tools list both.

The desktop binary compiles `tools.json` in with `include_str!` (`desktop/src/registry.rs`). The website still keeps its own copy of the registry in `src/config/tools.ts`, with the icon components; `src/lib/tools.test.ts` fails when the two disagree, and a desktop test asserts 37 tools in 9 categories with unique slugs.

## Desktop app

```
desktop/
├── Cargo.toml            pinned dependencies, one section per tool family
├── build.rs              compiles the Slint UI
├── assets/               app icon
├── scripts/              fetch-pdfium.sh, fetch-pdfium.ps1
├── src/
│   ├── main.rs           window, sidebar, tool selection, file drops, backdrop
│   ├── registry.rs       tools.json loaded into Tool / Category / Platform
│   ├── jobs.rs           background work and cancellation
│   ├── files.rs          file dialogs and output reservation
│   ├── file_queue.rs     per-tool file lists
│   ├── select.rs, clipboard.rs
│   ├── tools/            one module per tool
│   └── engine/           shared engines, free of UI
└── ui/
    ├── app.slint         the shell
    ├── theme.slint       design tokens
    ├── components/       sidebar, controls, fields, editor, file list, empty state
    ├── tools/            one .slint view per tool
    ├── icons/            Lucide icons (ISC) and icons.slint
    └── fonts/            Inter and JetBrains Mono (OFL)
```

### Tools

Each tool is a module in `src/tools/` with the same interface, declared once in the `tools!` macro in `src/tools/mod.rs`:

- `SLUG`, the slug in `tools.json`.
- `init(app)`, which wires the tool's Slint global.
- `set_drop_hover(app, hover)` and `add_paths(app, paths)`, for files dragged over or dropped on the window. Only the tool on screen gets drops.
- `shutdown()`, which cancels background work and returns its threads.

The macro generates `init_all`, `shutdown_all` (cancel every tool first, then join all threads together, so no partial output outlives the app), and the routing of drops by slug. Adding a tool means a module, a `.slint` view, and a line in the macro list.

### Engines

`src/engine/` holds the work behind the tools, free of UI so it is tested without a window. A family puts logic here once two or more of its tools share it.

| Module | Used by | Built on |
| --- | --- | --- |
| `ffmpeg`, `ffmpeg_audio`, `ffmpeg_video` | audio and video tools | the `ffmpeg` and `ffprobe` programs |
| `pdf` | the seven PDF tools | `pdfium-render` (PDFium) and `lopdf` |
| `image` | image converter and cropper, palette extractor | the `image` crate, `webp` |
| `svg` | SVG optimizer, pattern generator, border animator | `resvg`, `oxvg_optimiser` |
| `text` | text tools | `similar`, `pulldown-cmark`, `serde_json` |
| `archive` | archive extractor and archiver | `zip`, `tar`, `flate2`, `sevenz-rust2` |
| `color` | the colour tools | colour parsing and a gradient renderer ported from the website |

### Jobs

The UI thread never blocks. Tools hand work to `jobs::run_pool`, which runs items on at most N worker threads, and post results back with `jobs::post`, which runs a closure on the Slint event loop. Each run of a row gets a fresh `Arc<CancelToken>` kept on the row; a result is applied only if the row still holds that same token, so a stale worker cannot overwrite a newer run. Rows are addressed by a stable id, never by index. A `CancelToken` can also hold a child process, so cancelling kills FFmpeg.

### Output files

`files.rs` hands out `Reserved` files. `reserve_output` creates a new file beside the source or in a chosen folder with `create_new`, taking `name (1).ext` when a name is in use. `reserve_replacing` is for a path picked in a save dialog: the job writes a temporary file beside it, and only `keep` moves it over the user's file, after the job succeeded. Dropping a `Reserved` without `keep` deletes the file, so a failed, cancelled or panicked job leaves nothing behind.

### Finding FFmpeg and PDFium

FFmpeg is not bundled. `Ffmpeg::shared()` locates it once (`engine/ffmpeg.rs`): next to the executable, then in `desktop/vendor/<os>/` (debug builds only), then on `PATH`. `ffmpeg` and `ffprobe` must be in the same directory. When neither is found, each audio and video tool shows an "FFmpeg wasn't found" screen with a **Get FFmpeg** button that opens ffmpeg.org's download page.

PDFium is bundled. `pdf::pdfium()` binds the library once (`engine/pdf.rs`), looking next to the executable, then in `desktop/vendor/<os>/pdfium/` in debug builds. `scripts/fetch-pdfium.sh` downloads a pinned build from bblanchon/pdfium-binaries, checks its SHA-256 and unpacks it there with its licence files. If the library is missing, the PDF tools show a screen that links to `PDFIUM_DOWNLOADS`.

### Security gates

All input is untrusted. The gates fail closed:

- **FFmpeg input.** `Ffmpeg::probe` refuses anything that is not a single local media file (`ALLOWED_DEMUXERS`), and `run` probes again right before starting. Input reaches FFmpeg only through `input_args`.
- **FFmpeg arguments.** `engine/ffmpeg/args.rs` holds allowlists. Output options must be on `OUTPUT_OPTIONS` with the value FFmpeg reads for each; filtergraphs may use only listed filters, none of which reads a file; `Filter`, `Chain` and `Graph` escape values, and `check_graph` parses the result as FFmpeg does. `run` derives the muxer from the output extension and writes one reserved file; an image sequence goes into a reserved folder under a name pattern built in `engine/ffmpeg/frames.rs`.
- **Archives.** `engine/archive.rs` refuses the whole archive and deletes what it wrote on a path-escaping or reserved name, too many entries or bytes, or a repeated name. Links and device files are never created.
- **SVG.** `engine/svg.rs` resolves `<image>` and `<feImage>` to nothing, uses only the bundled Inter font, and caps input and render sizes.
- **PDF and images.** PDFs are opened only as regular local files. Page renders, PDF stream decompression and image decodes are capped, so a small hostile file cannot exhaust memory.

### UI

The UI is Slint. `ui/theme.slint` defines two tiers of tokens, raw palette steps and semantic `Theme` roles, each defined for light and dark; components read `Theme` only. One blue accent drives interactive states. The window uses the winit backend (required for file drops) and applies Mica on Windows and vibrancy on macOS where the platform allows it.

## Web app

```
src/
├── app/
│   ├── (tools)/<slug>/     one folder per tool: layout.tsx and page.tsx
│   ├── docs/page.tsx       the documentation page
│   ├── page.tsx            the landing page
│   ├── layout.tsx, not-found.tsx, opengraph-image.tsx, robots.ts, sitemap.ts
├── components/             common/, site/ (shell, PlatformGate, observability), tools/<slug>/, ui/
├── config/
│   ├── tools.ts            the registry with icon components, and runsOnWeb
│   ├── site.ts             website constants
│   └── security-headers.ts the Content Security Policy and companions
├── data/inDevelopmentSlugs.ts
└── lib/                    ffmpeg/, converters/, pdf/, design-tools/, audio/, tools/<slug>/
```

Every page is prerendered. The media engines run in the browser as WebAssembly: FFmpeg (`@ffmpeg/core`) and ImageMagick (`@imagemagick/magick-wasm`), plus the pdf.js worker. `scripts/copy-vendor.mjs` copies them from `node_modules` into `public/vendor/` (run by `npm run dev` and `npm run build`), so the app never fetches them from a CDN. Because the engine is wasm, the web audio and video tools need no FFmpeg install.

`src/config/security-headers.ts` builds the Content Security Policy and the other response headers, applied to every route in `next.config.ts`. The policy allows `'wasm-unsafe-eval'` for the wasm engines and `blob:` for the FFmpeg worker, previews and downloads, and closes everything else: no plugins, no `<base>`, no framing, no foreign script, worker or connect origin beyond `api.github.com` (Directory Scaffold) and Better Stack. `'unsafe-inline'` is allowed for scripts because prerendered Next pages write their data as inline scripts. The `/directory-scaffold` page loads no foreign script.

## CI and release

`.github/workflows/ci.yml` runs on pushes to `main`, `feat/**` and `wip/**` and on pull requests:

- **desktop** (Windows): installs FFmpeg with Chocolatey, fetches PDFium, then `cargo fmt --check`, rustfmt on every tracked `.rs` file (because `cargo fmt` skips modules declared inside the `tools!` macro), `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked`. It uploads `Cargo.lock` as an artifact.
- **check** (Linux, Node 24): `npm run check:ci`, `npm test` and `npm audit --omit=dev --audit-level=high`.
- **e2e-web** (Linux): installs Chromium and runs `npm run test:e2e:web` against a production build.

`.github/workflows/lockfile.yml` regenerates `package-lock.json` as an artifact. Dependabot opens weekly npm and monthly GitHub Actions updates.

A release is a pushed tag `vX.Y.Z` that matches the version in `desktop/Cargo.toml` and `package.json`. `.github/workflows/release.yml` builds the installers, writes `SHA256SUMS.txt`, attaches a build provenance attestation and publishes a GitHub Release. A manual run of that workflow makes a private draft build instead. UNKNOWN: the workflow's internal steps, which this document does not describe.
