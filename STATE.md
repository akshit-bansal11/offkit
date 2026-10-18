# STATE.md — Offkit

**Updated:** 2026-10-18 · **Branch:** `main` · **Released:** `v0.2.0` (tag on `f01e39d`)

## Bootstrap record

| Field | Value |
| --- | --- |
| Bootstrapped | 2026-10-18 (continuity files added with the 0.2.0 release; the code predates them) |
| Stack detected as | Rust 2024 + Slint 1.18.1 desktop app (`desktop/`), Next.js 16 web app (`src/`), shared `tools.json` — see `TECH-STACK.md` |
| Quality gate command | Web `npm run check:ci`; desktop `cargo fmt --check` + `cargo clippy --locked --all-targets -D warnings` — **run in CI only** |
| Deployed at | Web: `https://open-tools.akshitbansal.me` (resolves). `https://offkit.akshitbansal.me` does **not** resolve yet. Desktop: GitHub Releases `v0.2.0` |

## Where the work is right now

- **In flight:** nothing. The `v0.2.0` release run (39118463064) succeeded and the release is published as Latest.
- **Blocked on:** the owner, for the Vercel rename and the new domain (`OPEN_ITEMS.md`).
- **Next action:** the owner items in `OPEN_ITEMS.md`. Dependabot #20 (actions/setup-node 7) and #21 (diff 9) merged on 2026-10-19 after passing CI on the rebased `main`.

## Verified facts

| Fact | Proved by | Verified |
| --- | --- | --- |
| All four targets build and package (exe, two dmg, AppImage, deb) | Release run 39116733197 (draft `test-build-5`), all jobs success | 2026-10-18 |
| The deb puts `libpdfium.so`, notices and licences in `usr/lib/offkit/`, which `library_dirs()` searches as `../lib/offkit` from `usr/bin` | Unpacked `offkit-0.2.0-draft.5-amd64.deb` | 2026-10-18 |
| Desktop fmt, clippy and tests pass; web lint, unit and e2e tests pass | CI run 39114734441 on the released tree's parent, plus `f01e39d` adding only the `.icns` | 2026-10-18 |
| The desktop UI renders in both themes with version 0.2.0 and the AboutSlint badge | `screenshots.yml` run 39113390794 artifact | 2026-10-18 |
| `main` is unprotected | `gh api repos/akshit-bansal11/offkit/branches/main/protection` → 404 | 2026-10-18 |

## Standing hazards

| Hazard | Why it is dangerous here | Safe alternative |
| --- | --- | --- |
| Any local `cargo`, `npm`, `npx`, `node` | Concurrent local builds crashed the owner's laptop | Push a branch; CI and `lockfile.yml` do the work |
| Tagging `v*` | Publishes a public GitHub Release with provenance | Run `release.yml` by hand first: it makes a private draft |
| Hand-editing JSX text or long attributes | Biome reflows to 80 columns and `biome ci` fails on any difference | Match Biome's fill exactly; let CI confirm |

## Environment state

| Thing | State | Verified |
| --- | --- | --- |
| Branch protection on `main` | off | 2026-10-18 |
| Required status checks | none enforced | 2026-10-18 |
| Release approval environment | none; releases need no secrets (unsigned / ad hoc signed) | 2026-10-18 |
| Secrets present in CI | none needed | 2026-10-18 |
| Vercel project | still named `open-tools`; only `open-tools.akshitbansal.me` attached | 2026-10-18 |

## Test status, honestly

| Suite | Status | Note |
| --- | --- | --- |
| Desktop unit (`cargo test`, Windows) | green | Media tests need ffmpeg, which CI installs via Chocolatey |
| Web unit (`node --test`) | green | |
| Browser (Playwright) | green | |
| Desktop on macOS / Linux | **never run** | Built and packaged in CI; nobody has launched them |
| Accessibility scan | never run | Contrast is measured by script in `theme.slint` / `globals.css` comments |

## Continuity file health

| File | Exists | Last updated | Current? |
| --- | --- | --- | --- |
| `TECH-STACK.md` | yes | 2026-10-18 | yes |
| `DIRECTORY-STRUCTURE.md` | no | — | layout is in `docs/ARCHITECTURE.md` |
| `DECISIONS.md` | no | — | — |
| `DRIFT.md` | no | — | — |
| `OPEN_ITEMS.md` | yes | 2026-10-18 | yes |

## Session handoff

- **Do not touch:** nothing is mid-flight.
- **Half-applied changes:** none.
- **Left behind:** the draft release `test-build-5` (no tag behind it). Deleting it was not permitted from the agent session. The `feat/release-0.2.0` and `lucide-fix` branches and the worktree are already gone (checked 2026-10-19).
- **Open questions:** see `OPEN_ITEMS.md` (owner: Vercel rename, domain, repo homepage).
