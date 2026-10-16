// site.ts
// Global site metadata, author info, download and social links.

const REPOSITORY_URL = "https://github.com/akshit-bansal11/open-tools";

export const siteConfig = {
  name: "open-tools",
  url: "https://open-tools.akshitbansal.me",
  description:
    "Free tools for images, video, audio, PDFs, SVG, colors and text, in your browser or as a Windows app. Files are processed on your device and never uploaded.",
  repositoryUrl: REPOSITORY_URL,
  // Served by src/app/opengraph-image.tsx. A route that sets its own openGraph
  // object replaces the inherited one, so each such route lists this image.
  socialImage: {
    url: "/opengraph-image",
    width: 1200,
    height: 630,
    alt: "Open Tools: file tools that run on your device",
  },
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
