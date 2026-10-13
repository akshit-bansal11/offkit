// opengraph-image.web.tsx
// Social card for the website build, generated at build time.

import { ImageResponse } from "next/og";

export const alt = "Open Tools: file tools that run on your PC";
export const size = { width: 1200, height: 630 };
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
      <div style={{ fontSize: 120 }}>Open Tools</div>
      <div style={{ fontSize: 40, color: "#a3a3a3" }}>
        File tools that run on your PC. Nothing is uploaded.
      </div>
    </div>,
    size,
  );
}
