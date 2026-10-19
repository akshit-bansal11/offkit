// SiteShell.tsx
// Page frame for the website build: skip link, header nav, main landmark and footer.

import Link from "next/link";
import type { ReactNode } from "react";
import { GithubMark } from "@/components/common/GithubMark";
import { Button } from "@/components/ui/Button";
import { siteConfig } from "@/config/site";

export function SiteShell({ children }: { children: ReactNode }) {
  return (
    <div className="page-shell">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-50 focus:rounded-md focus:bg-background focus:px-4 focus:py-2 focus:text-foreground"
      >
        Skip to content
      </a>
      <header className="page-main relative z-10">
        <nav
          aria-label="Site"
          className="flex flex-wrap items-center justify-between gap-4"
        >
          <Link
            href="/"
            className="rounded-md text-lg font-semibold tracking-tight outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Offkit
          </Link>
          <ul className="flex flex-wrap items-center gap-2">
            <li>
              <Button asChild variant="ghost" size="sm">
                <Link href="/docs">Docs</Link>
              </Button>
            </li>
            <li>
              <Button asChild variant="ghost" size="sm">
                <a href={siteConfig.repositoryUrl}>
                  <GithubMark />
                  GitHub
                </a>
              </Button>
            </li>
          </ul>
        </nav>
      </header>
      <main id="main" className="page-main relative z-10">
        {children}
      </main>
      <footer className="page-main relative z-10 text-sm text-muted-foreground">
        <p>
          Offkit is open source.{" "}
          <a
            className="rounded-sm underline underline-offset-4 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            href={siteConfig.repositoryUrl}
          >
            Read the code on GitHub
          </a>
          .
        </p>
      </footer>
    </div>
  );
}
