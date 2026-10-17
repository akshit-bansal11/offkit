// opengraph-image.tsx
// Social card, generated at build time. Also emitted, unused, by the desktop export.

import { ImageResponse } from "next/og";
import { siteConfig } from "@/config/site";

// The desktop build is a static export, which only accepts metadata routes that are fully static.
export const dynamic = "force-static";

export const alt = siteConfig.socialImage.alt;
export const size = {
  width: siteConfig.socialImage.width,
  height: siteConfig.socialImage.height,
};
export const contentType = "image/png";

export default function Image() {
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        // Same value .page-shell uses in globals.css.
        background: "#080808",
        color: "#ffffff",
      }}
    >
      <div style={{ fontSize: 120 }}>Offkit</div>
      <div style={{ fontSize: 40, color: "#a3a3a3" }}>
        File tools that run on your device. Nothing is uploaded.
      </div>
    </div>,
    size,
  );
}
