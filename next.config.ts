import type { NextConfig } from "next";
import path from "path";
import { fileURLToPath } from "url";

const repoRoot = path.dirname(fileURLToPath(import.meta.url));

const nextConfig: NextConfig = {
  reactCompiler: true,
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
