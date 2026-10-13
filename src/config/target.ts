// target.ts
// Which product this build is: the desktop app (the tools) or the website (landing page and docs).

export const IS_DESKTOP = process.env.BUILD_TARGET === "desktop";
