// page.tsx
// Documentation page: install steps, privacy, per-tool reference with a link to each tool, and known limits.

import type { Metadata } from "next";
import Link from "next/link";
import { SiteShell } from "@/components/site/SiteShell";
import { siteConfig } from "@/config/site";
import { runsOnWeb, toolsByCategory } from "@/config/tools";

const TITLE = "Documentation";
const DESCRIPTION =
  "How to use Offkit in your browser or install the desktop app on Windows, macOS or Linux, what each tool does, and its known limits.";

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
              <a className={link} href={siteConfig.download.latest}>
                the latest release
              </a>
              .
            </li>
            <li>
              Windows: run offkit-&lt;version&gt;.exe. SmartScreen may show
              &quot;Windows protected your PC&quot; because the installer is not
              code-signed. Choose More info, then Run anyway. Open Offkit from
              the Start menu.
            </li>
            <li>
              macOS: open offkit-&lt;version&gt;-arm64.dmg (Apple Silicon) or
              offkit-&lt;version&gt;-x64.dmg (Intel) and drag Offkit to
              Applications. The app is ad hoc signed and not notarised, so on
              first launch right-click it and choose Open, or go to System
              Settings, then Privacy &amp; Security, and choose Open Anyway.
            </li>
            <li>
              Linux: run offkit-&lt;version&gt;-x86_64.AppImage, or install
              offkit-&lt;version&gt;-amd64.deb.
            </li>
          </ol>
          <p className={body}>
            Each release also lists SHA256SUMS.txt for checking your download.
          </p>
          <p className={body}>
            The audio and video tools need FFmpeg and ffprobe installed, because
            Offkit does not bundle them. Use winget install Gyan.FFmpeg on
            Windows, brew install ffmpeg on macOS, or sudo apt install ffmpeg on
            Linux. Until then the app shows a Get FFmpeg screen. PDFium for the
            PDF tools is bundled.
          </p>
          <p className={body}>
            The app does not update itself. To update, download the newest
            release and install it again. To uninstall on Windows, open
            Settings, then Apps, then Installed apps, find Offkit and choose
            Uninstall.
          </p>
        </section>

        <section aria-labelledby="privacy">
          <h2 id="privacy" className={h2}>
            Privacy
          </h2>
          <p className={body}>
            Every tool works on files on your own device, in the browser and in
            the desktop app alike. Nothing is uploaded and there is no account.
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
            Better Stack&apos;s script does not load on the Directory Scaffold
            page. The desktop app makes no network requests of its own and sends
            no analytics or error reports. It only opens a link in your browser
            when you click one.
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
              In the browser, video can be encoded as H.264, MPEG-4, VP8,
              ProRes, FLV and Theora; H.265 and VP9 are not in its FFmpeg build.
              The desktop app offers whatever encoders the FFmpeg you installed
              has.
            </li>
            <li>
              In the browser, media tools hold the whole file in memory, so very
              large videos can fail. Try a shorter clip or a smaller file.
            </li>
          </ul>
        </section>
      </div>
    </SiteShell>
  );
}
