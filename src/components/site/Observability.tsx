// Observability.tsx
// Better Stack error reporting, loaded after the page is interactive.

import Script from "next/script";

// A public application token: it says where reports go and grants nothing.
const ERRORS_TOKEN = process.env.NEXT_PUBLIC_BETTER_STACK_ERRORS_TOKEN ?? "";

/**
 * Reports uncaught errors only. Files a visitor opens in a tool are never read
 * by this script. Renders nothing without a token, so local runs and CI stay
 * silent.
 */
export function Observability() {
  if (!ERRORS_TOKEN) return null;

  return (
    <Script id="better-stack-errors" strategy="afterInteractive">
      {`!function(b,e,t,r){b[t]=b[t]||function(...args){(b[t].q=b[t].q||[]).push(args)};b[t].l=+new Date;var s=e.createElement('script');s.async=1;s.crossOrigin='anonymous';s.src='https://betterstack.net/b.js?t='+r;(e.head||e.getElementsByTagName('head')[0]).appendChild(s)}(window,document,'betterstack',${JSON.stringify(ERRORS_TOKEN)});betterstack('init',{environment:${JSON.stringify(process.env.NODE_ENV)}});`}
    </Script>
  );
}
