// tool.ts
// Shared type definitions for tool and category registry entries.

import type { LucideIcon } from "lucide-react";

export type Platform = "web" | "desktop";

export interface ToolDefinition {
  slug: string;
  /** Where the tool runs. A tool can be web-only or desktop-only. */
  platforms: Platform[];
  href: `/${string}`;
  name: string;
  shortName?: string;
  description: string;
  category: string;
  highlights: string[];
  icon: LucideIcon;
}

export interface ToolCategoryDefinition {
  name: string;
  icon: LucideIcon;
  accentColor?: string;
}
