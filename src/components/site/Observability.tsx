"use client";

// Observability.tsx
// Better Stack error reporting, loaded after the page is interactive, on every page but those in UNOBSERVED_PATHS.

import { usePathname } from "next/navigation";
import Script from "next/script";
import { useEffect } from "react";
import {
  BETTER_STACK_SCRIPT_ORIGIN,
  UNOBSERVED_PATHS,
} from "@/config/security-headers";

// A public application token: it says where reports go and grants nothing.
const ERRORS_TOKEN = process.env.NEXT_PUBLIC_BETTER_STACK_ERRORS_TOKEN ?? "";

/**
 * Sends uncaught errors and page-level telemetry (page URL, timing) to Better
 * Stack. It does not read files a visitor opens in a tool, which stay on the
 * device. Renders nothing without a token, so local runs and CI stay silent.
 */
export function Observability() {
  const pathname = usePathname();
  const unobserved = UNOBSERVED_PATHS.includes(pathname);

  useEffect(() => {
    // A loaded script cannot be unloaded. Arriving at Git Scaffold by an
    // in-app link after another page started it, a full load starts clean.
    if (unobserved && "betterstack" in window) window.location.reload();
  }, [unobserved]);

  if (!ERRORS_TOKEN || unobserved) return null;

  return (
    <Script id="better-stack-errors" strategy="afterInteractive">
      {`!function(b,e,t,r){b[t]=b[t]||function(...args){(b[t].q=b[t].q||[]).push(args)};b[t].l=+new Date;var s=e.createElement('script');s.async=1;s.crossOrigin='anonymous';s.src=${JSON.stringify(`${BETTER_STACK_SCRIPT_ORIGIN}/b.js?t=`)}+r;(e.head||e.getElementsByTagName('head')[0]).appendChild(s)}(window,document,'betterstack',${JSON.stringify(ERRORS_TOKEN)});betterstack('init',{environment:${JSON.stringify(process.env.NODE_ENV)}});`}
    </Script>
  );
}
