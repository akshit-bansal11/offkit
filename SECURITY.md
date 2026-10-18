# Security

Offkit opens files you did not write: media, PDFs, SVGs, archives. Its boundaries matter. This is what they are and how to report a hole in one.

## The model

- **Your files stay on your machine.** The desktop app has no upload code, no account and no analytics. It reads the files you choose or drop and writes results beside them or into a folder you pick. The only thing it sends anywhere is a link it hands to your default browser: "Open in Browser" for a web-only tool (an `https://` address built from `siteUrl` in `tools.json`), and the FFmpeg download page.
- **Every output is a new file.** Jobs create their output with `create_new`, taking `name (1).ext` when a name is in use, and delete it if they fail, are cancelled or panic. A job replaces an existing file only when you picked that exact path in a save dialog, and only after it has succeeded (`desktop/src/files.rs`).
- **FFmpeg is fed through two gates.** `Ffmpeg::probe` refuses anything that is not a single local media file, using an allowlist of demuxers, and `run` probes every input again right before it starts, so a file swapped after it was added cannot slip through. What a tool asks FFmpeg to do goes through allowlists of output options and filters; anything else is refused before FFmpeg starts, including options that name or read a file (`desktop/src/engine/ffmpeg.rs`, `desktop/src/engine/ffmpeg/args.rs`).
- **Archives are treated as hostile.** Extraction refuses the whole archive, and deletes what it already wrote, on a name that could land outside the folder (`..`, an absolute path, a drive letter, a backslash), a name Windows reserves, a `:`, too many entries or too many unpacked bytes, or a duplicate name. Links and device files are never created (`desktop/src/engine/archive.rs`).
- **SVGs cannot read your disk.** SVG previews resolve `<image>` and `<feImage>` references to nothing, so a hostile SVG cannot pull in local files, and input and render sizes are capped (`desktop/src/engine/svg.rs`).
- **PDFs and images are size-capped.** PDF pages render under a pixel cap, PDF streams decompress under a byte cap, and image decodes are limited in size, so a small file claiming to be enormous is refused instead of exhausting memory.
- **The web app is a static site.** Every page is served with a Content Security Policy that allows no foreign script, worker or frame, no plugins and no `<base>`, plus `X-Content-Type-Options`, `Referrer-Policy`, `Permissions-Policy` and `Strict-Transport-Security` (`src/config/security-headers.ts`). The one tool that goes online is Directory Scaffold on the website, which reads repository trees from `api.github.com`. The website loads Vercel Web Analytics and Better Stack error reporting; the Better Stack script is kept off the Directory Scaffold page, which can hold a GitHub token.
- **No secrets in the repository.** Releases are built by a GitHub Actions workflow and carry a build provenance attestation.

## Known limits

- **The Windows installer is not code-signed.** SmartScreen warns on first run. Check the download against `SHA256SUMS.txt` and with `gh attestation verify offkit-<version>.exe --repo akshit-bansal11/offkit`.
- **The macOS app is ad hoc signed and not notarised.** macOS refuses it on first launch until you right-click it and choose Open, or choose Open Anyway in System Settings > Privacy & Security. Verify the dmg the same way as the exe.
- **FFmpeg and ffprobe are not bundled.** The desktop app runs the `ffmpeg` and `ffprobe` it finds, looking next to the Offkit executable first and then on `PATH`. Whatever binary is there is what runs, so install FFmpeg from a source you trust and keep your `PATH` clean.
- **PDFium is bundled** and is a native library that parses untrusted PDFs. A flaw in it is a flaw in the PDF tools until the bundled build is updated.
- **The desktop app does not update itself.** A fix reaches you when you install a newer release.
- **Dependencies are third-party code.** Rust crates are pinned with `=` in `desktop/Cargo.toml`, and CI runs `npm audit --omit=dev --audit-level=high` for the web app. Dependabot opens update PRs.

## Reporting

Open a [security advisory](https://github.com/akshit-bansal11/offkit/security/advisories/new) on GitHub; it reaches the maintainer privately. Please do not open a public issue for something exploitable. UNKNOWN: there is no stated response time yet.
