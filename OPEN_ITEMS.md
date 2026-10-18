# OPEN_ITEMS.md — Offkit

Deferred, accepted or unprovisioned work. Delete an entry when it is done; never strike it through.

| Item | Why it is open | Who / what unblocks it |
| --- | --- | --- |
| Vercel project rename `open-tools` → `offkit` | The Vercel connector answered 403 on project update (2026-10-18). | Owner: Vercel dashboard → Project Settings → Name. |
| `offkit.akshitbansal.me` on the website | DNS is at Namecheap; adding the domain was not permitted from the agent session. `src/config/site.ts`, `tools.json` and the README already point there. | Owner: add a `CNAME offkit → cname.vercel-dns.com` at Namecheap, add the domain to the Vercel project, then set `open-tools.akshitbansal.me` to redirect (308) to it. |
| GitHub repo homepage | Still `https://use-open-tools.vercel.app/`. | Set it to `https://offkit.akshitbansal.me` once that domain resolves. |
| Tailwind 4 (Dependabot #17) | A migration (config moves into CSS), not a version bump. Dependabot ignores the major. | A dedicated change, then drop the ignore in `.github/dependabot.yml`. |
| TypeScript 7 (Dependabot #19) | typescript-eslint does not support TypeScript 7 yet. Dependabot ignores the major. | Drop the ignore when typescript-eslint supports TS 7. |
| Code signing | The Windows installer is unsigned and the macOS app is ad hoc signed, not notarised, so SmartScreen and Gatekeeper warn. | An Authenticode certificate and an Apple Developer ID, if ever wanted. |
| macOS and Linux run-time check | Both are built and packaged in CI, but nobody has launched them on a real machine. | Install the release on a Mac and a Linux box and open a PDF and a media tool. |
