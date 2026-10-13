// robots.ts
// Robots rules. Also emitted, unused, by the desktop export.

import type { MetadataRoute } from "next";
import { siteConfig } from "@/config/site";

// The desktop build is a static export, which only accepts metadata routes that are fully static.
export const dynamic = "force-static";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: { userAgent: "*", allow: "/" },
    sitemap: `${siteConfig.url}/sitemap.xml`,
  };
}
