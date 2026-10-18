// page.tsx
// Landing page: what Offkit is, every tool to open in the browser, and the desktop app download.

import { Download } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { CategoryCard } from "@/components/common/CategoryCard";
import { SiteShell } from "@/components/site/SiteShell";
import { Button } from "@/components/ui/Button";
import { siteConfig } from "@/config/site";
import { tools, toolsByCategory } from "@/config/tools";

const TITLE = "Offkit: file tools that run on your device";

export const metadata: Metadata = {
  title: { absolute: TITLE },
  description: siteConfig.description,
  alternates: { canonical: "/" },
  openGraph: {
    title: TITLE,
    description: siteConfig.description,
    url: "/",
    images: [siteConfig.socialImage],
  },
};

export default function Home() {
  return (
    <SiteShell>
      <div className="flex flex-col items-center gap-10">
        <section className="flex flex-col items-center p-8 sm:p-10">
          <h1 className="brand-title relative z-10 text-4xl font-semibold tracking-tight sm:text-8xl">
            Offkit
          </h1>
          <p className="mt-4 max-w-2xl text-center text-base leading-7 text-muted-foreground sm:text-lg">
            {tools.length} file tools that run on your device. Use them here in
            your browser, or download the desktop app for Windows, macOS and
            Linux. Your files are never uploaded.
          </p>
          <div className="mt-6 flex flex-wrap items-center justify-center gap-3">
            <Button asChild size="lg">
              <a href={siteConfig.download.latest}>
                <Download aria-hidden="true" />
                Download the desktop app
              </a>
            </Button>
            <Button
              asChild
              size="lg"
              variant="outline"
              className="glass-button"
            >
              <Link href="/docs">Read the docs</Link>
            </Button>
          </div>
          <p className="mt-4 text-center text-sm text-muted-foreground">
            Windows 10 or later (64-bit), macOS 11 or later, or 64-bit Linux.
          </p>
        </section>

        <section aria-labelledby="tools" className="w-full">
          <h2 id="tools" className="mb-6 text-2xl font-semibold tracking-tight">
            Tools
          </h2>
          <div className="grid w-full grid-cols-1 gap-5 md:grid-cols-2 2xl:grid-cols-3">
            {toolsByCategory.map((category) => (
              <CategoryCard
                key={category.name}
                category={category}
                tools={category.tools}
                headingLevel="h3"
              />
            ))}
          </div>
        </section>

        <section aria-labelledby="local" className="w-full">
          <h2 id="local" className="mb-6 text-2xl font-semibold tracking-tight">
            How it stays local
          </h2>
          <dl className="tool-card grid gap-6 p-6 md:grid-cols-3">
            <div>
              <dt className="font-medium">No uploads</dt>
              <dd className="mt-2 text-sm leading-6 text-muted-foreground">
                Your files are read and written on your own device by engines
                that run there, in the browser and in the app alike: FFmpeg,
                ImageMagick and pdf.js.
              </dd>
            </div>
            <div>
              <dt className="font-medium">No file traffic</dt>
              <dd className="mt-2 text-sm leading-6 text-muted-foreground">
                No tool sends your files anywhere. The one tool that goes online
                is Directory Scaffold, which asks GitHub for a repository&apos;s
                file list when you use it. The desktop app makes no network
                requests of its own.
              </dd>
            </div>
            <div>
              <dt className="font-medium">No account</dt>
              <dd className="mt-2 text-sm leading-6 text-muted-foreground">
                There is nothing to sign in to. This website counts page views
                and reports script errors, and never sees your files. The
                desktop app collects nothing.
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </SiteShell>
  );
}
