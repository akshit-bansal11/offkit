// sitemap.ts
// Sitemap: the landing page and the docs. Also emitted, unused, by the desktop export.

import type { MetadataRoute } from "next";
import { siteConfig } from "@/config/site";

// The desktop build is a static export, which only accepts metadata routes that are fully static.
export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  return ["/", "/docs"].map((path) => ({ url: `${siteConfig.url}${path}` }));
}
