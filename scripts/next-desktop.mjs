// next-desktop.mjs
// Runs the Next CLI with BUILD_TARGET=desktop, so the desktop scripts work the same in cmd, PowerShell and bash.

import { createRequire } from "node:module";

process.env.BUILD_TARGET = "desktop";
createRequire(import.meta.url)("next/dist/bin/next");
