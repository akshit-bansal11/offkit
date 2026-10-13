// page.web.tsx
// Documentation page: install steps, privacy, per-tool reference and known limits.

import type { Metadata } from "next";
import { SiteShell } from "@/components/site/SiteShell";
import { siteConfig } from "@/config/site";
import { toolsByCategory } from "@/config/tools";

export const metadata: Metadata = {
  title: "Documentation",
  description:
    "How to install Open Tools on Windows, what each tool does, and what the app can and cannot do offline.",
  alternates: { canonical: "/docs" },
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
            Every tool works on files on your own machine. Nothing is uploaded,
            there is no account, and the app sends no analytics or error
            reports.
          </p>
          <p className={body}>
            The app blocks outbound network requests. Git Scaffold is the one
            exception: it asks api.github.com for a repository&apos;s file list,
            and only when you run it. If you give it a GitHub token, the token
            is stored on your PC and sent only to GitHub.
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
                  <h4 className="font-medium">{tool.name}</h4>
                  <p className={body}>{tool.description}</p>
                  <ul className={`${body} list-disc pl-5`}>
                    {tool.highlights.map((highlight) => (
                      <li key={highlight}>{highlight}</li>
                    ))}
                  </ul>
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
            <li>Windows only. There is no macOS or Linux build.</li>
          </ul>
        </section>
      </div>
    </SiteShell>
  );
}
