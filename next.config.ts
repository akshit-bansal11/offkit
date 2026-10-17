import type { NextConfig } from "next";
import path from "path";
import { fileURLToPath } from "url";
import { securityHeaders } from "./src/config/security-headers";

const repoRoot = path.dirname(fileURLToPath(import.meta.url));
const nextConfig: NextConfig = {
  reactCompiler: true,
  // Git Scaffold became Directory Scaffold; keep the old URL working.
  redirects: async () => [
    {
      source: "/git-scaffold",
      destination: "/directory-scaffold",
      permanent: true,
    },
  ],
  headers: async () => [
    {
      source: "/:path*",
      headers: securityHeaders(process.env.NODE_ENV === "development"),
    },
  ],
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
