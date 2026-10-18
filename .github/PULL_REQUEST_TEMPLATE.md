## What changed

One sentence. What a reader of `git log` needs to know.

## Why

The problem this solves, or the issue it closes. If you rejected a real alternative, say what and why.

## Scope

- [ ] This PR does exactly one thing. Unrelated changes were split out.
- [ ] No refactor of code the change did not require.
- Files touched: <!-- list, or "see diff" if over 20 -->

## How it was verified

Paste the actual output, not a claim.

```bash
# web: npm run check:ci, npm test
# desktop: cargo fmt --check, cargo clippy --locked --all-targets -- -D warnings, cargo test --locked
```

- [ ] CI is green on this PR.
- [ ] A tool change was tried in the app, on the side it touches (desktop, web or both).

## Review checklist

**Correctness**

- [ ] Every new branch has a test, or the change is copy, style or config only.
- [ ] No skipped test without a stated reason.

**Gate**

- [ ] No new `eslint-disable`, `biome-ignore`, `@ts-ignore` or `#[allow(...)]` added to get green.
- [ ] TypeScript: no `any`, no `as` used to silence an error, no `console.log`.
- [ ] No unused variables, imports or files.

**Tools and registry**

- [ ] A new or renamed tool is in `tools.json` and `src/config/tools.ts`, with `platforms` set, and the two still match (`npm test`).
- [ ] A tool that handles files runs offline: no CDN, no network request.

**Security** (any file parsing, FFmpeg arguments, paths, headers or network use)

- [ ] Untrusted input is validated at the boundary and fails closed.
- [ ] No secret in the diff.
- [ ] No new dependency without a reason in this PR. Rust crates are pinned with `=`.

**Docs**

- [ ] README, `docs/ARCHITECTURE.md` or `CHANGELOG.md` updated if behaviour or setup changed.

## Commit convention

[Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/): `<type>[scope]: <description>`.

- Types: `feat`, `fix`, `build`, `chore`, `ci`, `docs`, `style`, `refactor`, `perf`, `test`.
- Breaking change: `!` before the colon, or a `BREAKING CHANGE:` footer.
- Imperative description, under 72 characters, no trailing period. One purpose per commit.

## Deferred

Anything found and deliberately not fixed here, with a reason.
