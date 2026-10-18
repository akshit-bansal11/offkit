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
        // Brand palette (see globals.css): ink background, paper text, citrus accent.
        background: "#161a30",
        color: "#f6f7fb",
      }}
    >
      <div style={{ fontSize: 120, color: "#ffc23d" }}>Offkit</div>
      <div style={{ fontSize: 40, color: "#a9adc6" }}>
        File tools that run on your device. Nothing is uploaded.
      </div>
    </div>,
    size,
  );
}
