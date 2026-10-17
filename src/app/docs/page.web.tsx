// page.web.tsx
// Documentation page: install steps, privacy, per-tool reference with a link to each tool, and known limits.

import type { Metadata } from "next";
import Link from "next/link";
import { SiteShell } from "@/components/site/SiteShell";
import { siteConfig } from "@/config/site";
import { runsOnWeb, toolsByCategory } from "@/config/tools";

const TITLE = "Documentation";
const DESCRIPTION =
  "How to use Open Tools in your browser or install it on Windows, what each tool does, and its known limits.";

export const metadata: Metadata = {
  title: TITLE,
  description: DESCRIPTION,
  alternates: { canonical: "/docs" },
  openGraph: {
    title: TITLE,
    description: DESCRIPTION,
    url: "/docs",
    images: [siteConfig.socialImage],
  },
};

const link = "underline underline-offset-4";
const h2 = "mt-12 text-2xl font-semibold tracking-tight";
const body = "mt-4 leading-7 text-muted-foreground";

export default function DocsPage() {
  return (
    <SiteShell>
      <div className="mx-auto w-full max-w-3xl">
        <h1 className="text-4xl font-semibold tracking-tight">Documentation</h1>
        <nav aria-label="On this page" className="tool-card mt-6 p-4">
          <ul className="flex flex-wrap gap-x-6 gap-y-2 text-sm">
            <li>
              <a className={link} href="#install">
                Install
              </a>
            </li>
            <li>
              <a className={link} href="#privacy">
                Privacy
              </a>
            </li>
            <li>
              <a className={link} href="#tools">
                Tools
              </a>
            </li>
            <li>
              <a className={link} href="#limits">
                Known limits
              </a>
            </li>
          </ul>
        </nav>

        <section aria-labelledby="install">
          <h2 id="install" className={h2}>
            Install
          </h2>
          <ol className={`${body} list-decimal space-y-2 pl-5`}>
            <li>
              Download{" "}
              <a className={link} href={siteConfig.download.installer}>
                Open-Tools-Setup.exe
              </a>
              .
            </li>
            <li>
              Run it. Windows SmartScreen may show &quot;Windows protected your
              PC&quot; because the installer is not code-signed. Choose More
              info, then Run anyway.
            </li>
            <li>Open Open Tools from the Start menu.</li>
          </ol>
          <p className={body}>
            To use the app without installing it, download{" "}
            <a className={link} href={siteConfig.download.portable}>
              Open-Tools-Portable.exe
            </a>{" "}
            and run it from any folder.
          </p>
          <p className={body}>
            The app does not update itself. To update, download the newest
            installer and run it again.
          </p>
          <p className={body}>
            To uninstall, open Settings, then Apps, then Installed apps, find
            Open Tools and choose Uninstall.
          </p>
        </section>

        <section aria-labelledby="privacy">
          <h2 id="privacy" className={h2}>
            Privacy
          </h2>
          <p className={body}>
            Every tool works on files on your own device, in the browser and in
            the Windows app alike. Nothing is uploaded and there is no account.
          </p>
          <p className={body}>
            Directory Scaffold is the one tool that goes online: it asks
            api.github.com for a repository&apos;s file list, and only when you
            run it. A GitHub token you give it for a private repository is kept
            in memory on that page only. It is never saved, and it is sent only
            to api.github.com.
          </p>
          <p className={body}>
            This website counts page views with Vercel Web Analytics and reports
            script errors to Better Stack. Neither receives your files, and
            Better Stack&apos;s script does not load on the Directory Scaffold page.
            The Windows app blocks every other outbound request and sends no
            analytics or error reports.
          </p>
        </section>

        <section aria-labelledby="tools">
          <h2 id="tools" className={h2}>
            Tools
          </h2>
          {toolsByCategory.map((category) => (
            <div key={category.name}>
              <h3 className="mt-8 text-xl font-semibold">{category.name}</h3>
              {category.tools.map((tool) => (
                <article
                  key={tool.slug}
                  id={tool.slug}
                  className="tool-card mt-4 scroll-mt-6 p-5"
                >
                  <h4 className="font-medium">
                    {tool.name}
                    {!runsOnWeb(tool) && " (desktop app only)"}
                  </h4>
                  <p className={body}>{tool.description}</p>
                  <ul className={`${body} list-disc pl-5`}>
                    {tool.highlights.map((highlight) => (
                      <li key={highlight}>{highlight}</li>
                    ))}
                  </ul>
                  <p className="mt-4">
                    <Link className={link} href={tool.href}>
                      Open {tool.name}
                    </Link>
                  </p>
                </article>
              ))}
            </div>
          ))}
        </section>

        <section aria-labelledby="limits">
          <h2 id="limits" className={h2}>
            Known limits
          </h2>
          <ul className={`${body} list-disc space-y-2 pl-5`}>
            <li>
              Video can be encoded as H.264, MPEG-4, VP8, ProRes, FLV and
              Theora. H.265 and VP9 are not available in the bundled FFmpeg
              build.
            </li>
            <li>
              Media tools hold the whole file in memory, so very large videos
              can fail. Try a shorter clip or a smaller file.
            </li>
            <li>
              The desktop app is Windows only for now. On macOS and Linux, use
              the tools in your browser.
            </li>
          </ul>
        </section>
      </div>
    </SiteShell>
  );
}
