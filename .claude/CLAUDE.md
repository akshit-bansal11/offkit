This project inherits every rule in F:/.claude/CLAUDE.md. Rules below add to or override it. Nothing here restates it.

- **Never build or run anything locally:** no `cargo`, `rustc`, `npm`, `npx`, `node` or `next`, in the main session or any subagent. Concurrent local builds crashed the owner's laptop. Push a branch and let CI (`ci.yml`, `lockfile.yml`, `screenshots.yml`) be the gate; regenerate `Cargo.lock` / `package-lock.json` through `lockfile.yml`. Python is fine.
- **Releases:** bump `desktop/Cargo.toml` and `package.json` together, add the `CHANGELOG.md` section, then push tag `vX.Y.Z`. `release.yml` refuses a tag that disagrees with either version. A manual run of `release.yml` makes a private draft for testing.
- **Seeing the desktop UI:** run `screenshots.yml`; it renders tools in light and dark through the `OFFKIT_SNAPSHOT` hook in `desktop/src/main.rs`.
- **Colours:** desktop tokens live in `desktop/ui/theme.slint`, web tokens in `src/app/globals.css`. Every colour comment carries a measured WCAG ratio; recompute it when a value changes.
- **Edgepad** (`akshit-bansal11/edgepad`) is the owner's reference for release, docs and GitHub hygiene.
