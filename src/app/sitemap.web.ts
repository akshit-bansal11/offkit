// sitemap.web.ts
// Sitemap for the website build: the landing page and the docs.

import type { MetadataRoute } from "next";
import { siteConfig } from "@/config/site";

export default function sitemap(): MetadataRoute.Sitemap {
  return ["/", "/docs"].map((path) => ({ url: `${siteConfig.url}${path}` }));
}
