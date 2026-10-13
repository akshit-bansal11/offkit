// site.ts
// Global site metadata, author info, download and social links.

const REPOSITORY_URL = "https://github.com/akshit-bansal11/open-tools";

export const siteConfig = {
  name: "open-tools",
  url: "https://open-tools.akshitbansal.me",
  description:
    "Desktop utilities for images, video, audio, PDFs, SVG, colors and text. Files are processed on your PC and never uploaded.",
  repositoryUrl: REPOSITORY_URL,
  // File names are fixed by electron-builder.yml so these links always point at the newest release.
  download: {
    installer: `${REPOSITORY_URL}/releases/latest/download/Open-Tools-Setup.exe`,
    portable: `${REPOSITORY_URL}/releases/latest/download/Open-Tools-Portable.exe`,
  },
  author: {
    name: "Er. Akshit Bansal",
    role: "Next.js Developer",
  },
  social: {
    github: "https://github.com/akshit-bansal11",
    linkedin: "https://www.linkedin.com/in/akshit-bansal11/",
    email: "artistbansal2004@gmail.com",
  },
} as const;
