import type { NextConfig } from "next";
import path from "path";
import { fileURLToPath } from "url";
import { securityHeaders } from "./src/config/security-headers";
import { IS_DESKTOP } from "./src/config/target";

const repoRoot = path.dirname(fileURLToPath(import.meta.url));

// The desktop app is a static export loaded by Electron, which has no image optimizer.
const desktopConfig: NextConfig = {
  output: "export",
  pageExtensions: ["desktop.tsx", "tsx", "ts"],
  images: { unoptimized: true },
};

// A static export has no server to send headers; Electron sends its own policy.
const webConfig: NextConfig = {
  pageExtensions: ["web.tsx", "tsx", "ts"],
  headers: async () => [
    {
      source: "/:path*",
      headers: securityHeaders(process.env.NODE_ENV === "development"),
    },
  ],
};

// One codebase, two builds. Tool pages are plain page.tsx files, so both
// builds serve them. Only the home page differs: the desktop build picks up
// page.desktop.tsx (the launcher) and the web build page.web.tsx (the
// landing page) plus not-found.web.tsx and the docs.
// Metadata files (robots, sitemap, social image) keep plain names: Next did
// not pick them up under a compound extension.
const nextConfig: NextConfig = {
  reactCompiler: true,
  ...(IS_DESKTOP ? desktopConfig : webConfig),
  turbopack: {
    root: repoRoot,
    // JSZip's source build reaches Node's stream and util polyfills, whose
    // generator check calls Function() on load and trips the CSP's eval ban.
    // Its prebuilt browser bundle needs neither.
    resolveAlias: { jszip: "jszip/dist/jszip.min.js" },
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
