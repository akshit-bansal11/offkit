"use client";

// PlatformGate.tsx
// Wraps every tool page (see app/(tools)/layout.tsx). A tool that does not run on the web keeps
// its URL but shows a download panel in place of the tool.

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/Button";
import { siteConfig } from "@/config/site";
import { getToolBySlug, runsOnWeb } from "@/config/tools";

export function PlatformGate({ children }: { children: ReactNode }) {
  const slug = usePathname().split("/")[1] ?? "";
  const tool = getToolBySlug(slug);

  if (!tool || runsOnWeb(tool)) return children;

  return (
    <main className="page-main mx-auto flex max-w-xl flex-col items-center gap-4 py-24 text-center">
      <h1 className="text-3xl font-semibold tracking-tight">{tool.name}</h1>
      <h2 className="text-xl font-medium">Available in the desktop app</h2>
      <p className="text-muted-foreground">
        This tool needs the Offkit desktop app (Windows, macOS, Linux). Download
        it from the latest release.
      </p>
      <Button asChild>
        <a href={siteConfig.download.latest}>Download the desktop app</a>
      </Button>
      <Link className="text-sm underline" href="/">
        Back to all tools
      </Link>
    </main>
  );
}
