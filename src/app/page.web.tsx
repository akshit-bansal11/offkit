// page.web.tsx
// Landing page for the website build: what Open Tools is, where to download it, what is inside.

import { Download } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { CategoryCard } from "@/components/common/CategoryCard";
import { SiteShell } from "@/components/site/SiteShell";
import { Button } from "@/components/ui/Button";
import { siteConfig } from "@/config/site";
import { tools, toolsByCategory } from "@/config/tools";
import type { ToolDefinition } from "@/types/tool";

export const metadata: Metadata = {
  title: { absolute: "Open Tools: file tools that run on your PC" },
  description: siteConfig.description,
  alternates: { canonical: "/" },
};

const toolDocsHref = (tool: ToolDefinition) => `/docs#${tool.slug}`;

export default function Home() {
  return (
    <SiteShell>
      <div className="flex flex-col items-center gap-10">
        <section className="flex flex-col items-center p-8 sm:p-10">
          <h1 className="brand-title relative z-10 text-4xl font-semibold tracking-tight sm:text-8xl">
            Open Tools
          </h1>
          <p className="mt-4 max-w-2xl text-center text-base leading-7 text-muted-foreground sm:text-lg">
            {tools.length} file tools for Windows. Everything runs on your PC
            and nothing is uploaded.
          </p>
          <div className="mt-6 flex flex-wrap items-center justify-center gap-3">
            <Button asChild size="lg">
              <a href={siteConfig.download.installer}>
                <Download aria-hidden="true" />
                Download for Windows
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
            Windows 10 or later, 64-bit. Prefer no installer?{" "}
            <a
              className="underline underline-offset-4"
              href={siteConfig.download.portable}
            >
              Get the portable version
            </a>
            .
          </p>
        </section>

        <section aria-labelledby="inside" className="w-full">
          <h2
            id="inside"
            className="mb-6 text-2xl font-semibold tracking-tight"
          >
            What is inside
          </h2>
          <div className="grid w-full grid-cols-1 gap-5 md:grid-cols-2 2xl:grid-cols-3">
            {toolsByCategory.map((category) => (
              <CategoryCard
                key={category.name}
                category={category}
                tools={category.tools}
                toolHref={toolDocsHref}
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
                Your files are read and written on your own machine by engines
                that ship inside the app: FFmpeg, ImageMagick and pdf.js.
              </dd>
            </div>
            <div>
              <dt className="font-medium">No network</dt>
              <dd className="mt-2 text-sm leading-6 text-muted-foreground">
                The app blocks outbound requests. The one exception is Git
                Scaffold, which asks GitHub for a repository&apos;s file list
                when you use it.
              </dd>
            </div>
            <div>
              <dt className="font-medium">No account, no telemetry</dt>
              <dd className="mt-2 text-sm leading-6 text-muted-foreground">
                There is nothing to sign in to, and the app collects nothing
                about you or your files.
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </SiteShell>
  );
}
