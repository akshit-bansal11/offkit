// not-found.tsx
// 404 page.

import type { Metadata } from "next";
import Link from "next/link";
import { SiteShell } from "@/components/site/SiteShell";
import { Button } from "@/components/ui/Button";

export const metadata: Metadata = {
  title: "Page not found",
  robots: { index: false },
};

export default function NotFound() {
  return (
    <SiteShell>
      <h1 className="text-4xl font-semibold tracking-tight">Page not found</h1>
      <p className="mt-4 text-muted-foreground">
        There is nothing at this address.
      </p>
      <div className="mt-6 flex flex-wrap gap-3">
        <Button asChild>
          <Link href="/">Go to the home page</Link>
        </Button>
        <Button asChild variant="outline" className="glass-button">
          <Link href="/docs">Read the docs</Link>
        </Button>
      </div>
    </SiteShell>
  );
}
