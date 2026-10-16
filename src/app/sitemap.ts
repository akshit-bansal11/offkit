// sitemap.ts
// Sitemap: the landing page, the docs and every tool page, from the tool registry. Also emitted, unused, by the desktop export.

import type { MetadataRoute } from "next";
import { siteConfig } from "@/config/site";
import { tools } from "@/config/tools";

// The desktop build is a static export, which only accepts metadata routes that are fully static.
export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  return ["/", "/docs", ...tools.map((tool) => tool.href)].map((path) => ({
    url: `${siteConfig.url}${path}`,
  }));
}
