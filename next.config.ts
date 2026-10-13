import type { NextConfig } from "next";
import path from "path";
import { fileURLToPath } from "url";
import { IS_DESKTOP } from "./src/config/target";
import { tools } from "./src/config/tools";

const repoRoot = path.dirname(fileURLToPath(import.meta.url));

// The desktop app is a static export loaded by Electron, which has no image optimizer.
const desktopConfig: NextConfig = {
  output: "export",
  pageExtensions: ["desktop.tsx", "tsx", "ts"],
  images: { unoptimized: true },
};

// The tools left the website; their old URLs land on the matching docs entry.
const webConfig: NextConfig = {
  pageExtensions: ["web.tsx", "tsx", "ts"],
  redirects: async () =>
    tools.map((tool) => ({
      source: tool.href,
      destination: `/docs#${tool.slug}`,
      permanent: true,
    })),
};

// One codebase, two route sets: the desktop build only sees page.desktop.tsx
// files and the web build only sees page.web.tsx / not-found.web.tsx.
// Metadata files (robots, sitemap, social image) keep plain names: Next did
// not pick them up under a compound extension.
const nextConfig: NextConfig = {
  reactCompiler: true,
  ...(IS_DESKTOP ? desktopConfig : webConfig),
  turbopack: {
    root: repoRoot,
    // @ffmpeg/ffmpeg builds a Worker from a caller-supplied URL when
    // `classWorkerURL` is passed. This app never passes it, so that branch is
    // dead here, but Turbopack cannot resolve the dynamic URL and fails the
    // build on it.
    ignoreIssue: [
      {
        path: "**/node_modules/@ffmpeg/ffmpeg/dist/esm/classes.js",
        title: /Module not found/,
      },
    ],
  },
};

export default nextConfig;
