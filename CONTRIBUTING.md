# Contributing

Thanks for looking. Offkit is small on purpose; the bar for a change is that it makes the product better for someone using it, and that it passes the same checks CI runs.

## Before you start

- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). The two apps share `tools.json` and little else, and that split is deliberate.
- Open an issue for anything bigger than a fix, so the design can be talked through before code exists.
- Tools must work offline: no CDN, no network request.

## Setting up

Fork the repository on GitHub and clone your fork. Your work lives on your fork, and reaches this repository as a pull request.

```bash
git clone https://github.com/<you>/offkit.git
cd offkit
git remote add upstream https://github.com/akshit-bansal11/offkit.git
```

Either app can be worked on alone.

**Web app** (Next.js, `src/`): Node 24 (what CI uses).

```bash
npm install
npm run dev              # copies the wasm engines to public/vendor, then starts next dev
npm run check            # biome, eslint, stylelint, tsc; writes fixes
npm run check:ci         # the same checks without writing; what CI runs
npm test                 # unit tests (node --test)
npx playwright install chromium   # once
npm run test:e2e:web     # browser tests against a production build
```

**Desktop app** (Rust + Slint, `desktop/`): a current stable Rust toolchain with `rustfmt` and `clippy`. On Linux you also need `libasound2-dev` (audio preview); anything else the Slint backend needs there is UNKNOWN.

```bash
sh desktop/scripts/fetch-pdfium.sh     # once: PDFium into desktop/vendor/<os>/pdfium (on Windows: desktop/scripts/fetch-pdfium.ps1)
cd desktop
cargo run                              # starts the app
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The audio and video tools and their tests need `ffmpeg` and `ffprobe` on `PATH`; media tests skip without them. Debug builds also look for both in `desktop/vendor/<os>/`.

## Making a change

1. Bring your fork's `main` up to date with `upstream/main`, then branch from it on your fork.
2. Keep the two apps in step. A tool added, renamed or moved between platforms changes `tools.json` and `src/config/tools.ts` together; `npm test` fails if they differ, and the desktop registry test counts the tools. A desktop tool is a module in `desktop/src/tools/`, a view in `desktop/ui/tools/`, and an entry in the `tools!` list in `desktop/src/tools/mod.rs`. A web tool is a folder under `src/app/(tools)/<slug>/`.
3. Add or change a test when you add logic, a branch or fix a bug. Engine code in `desktop/src/engine/` is free of UI, so test there rather than through a window.
4. Run the checks for the app you changed until they are clean. Do not suppress a lint finding to get green: no `eslint-disable`, `biome-ignore`, `@ts-ignore` or `#[allow]` added to pass.
5. Commit with a [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) subject (`feat:`, `fix:`, `docs:`, `ci:`, with an optional scope such as `feat(desktop):`) and a body that says what changed and why, including what you rejected.
6. Add a line under **Unreleased** in `CHANGELOG.md` ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)).
7. Push the branch to your fork and open a pull request against `main`. CI runs `cargo fmt`, `clippy` and `test` on Windows and `npm run check:ci`, `npm test` and the Playwright suite for the web. The maintainer reviews and merges.

## Style

- No `any` and no `as` to silence a type error in TypeScript. No `console.log` in production code.
- Rust crates are pinned with `=` and must not be GPL or AGPL (the app is MIT). A new dependency, on either side, needs a reason in the pull request.
- Constants are named. A number that appears twice is a constant.
- Doc comments say why, not what the next line already says.
- Untrusted input fails closed. Never run FFmpeg with an argument that is not on the allowlists in `desktop/src/engine/ffmpeg/args.rs`, and never write an output over a file you did not create (`desktop/src/files.rs`).

## Versions and releases

Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The maintainer tags and publishes releases. A pull request does not bump the version, tag or build a release; a merged change ships in the next one.
