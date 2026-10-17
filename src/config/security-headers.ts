// security-headers.ts
// Response headers for the website build: the content security policy and its companions.

// Better Stack's error script. b.js is generated per application and names the
// host it reports to; this is the one the site's b.js names. If Better Stack
// moves the application, error reports are blocked until this value follows.
export const BETTER_STACK_SCRIPT_ORIGIN = "https://betterstack.net";
const BETTER_STACK_INGEST_ORIGIN =
  "https://s2791328.us-west-2a.betterstackdata.com";

// Directory Scaffold reads repository trees from here, and is the only tool that goes online.
const GITHUB_API_ORIGIN = "https://api.github.com";

// Pages that load no script from another origin. Directory Scaffold can hold a
// GitHub token, so Better Stack's script, which records sessions, stays off it.
// Vercel Web Analytics still runs there: it is served from this origin.
export const UNOBSERVED_PATHS: readonly string[] = ["/directory-scaffold"];

/**
 * The policy every page is served with.
 *
 * Scripts allow 'unsafe-inline' because every page is prerendered: Next
 * writes its flight data as inline scripts at build time, and a nonce needs a
 * fresh value per response, which only dynamic rendering can give. Static
 * pages are what keep the tools working from the CDN, so the policy closes
 * the other doors instead: no plugins, no <base>, no framing, no foreign
 * script, worker or connect origins.
 *
 * 'wasm-unsafe-eval' lets FFmpeg and ImageMagick compile their WebAssembly.
 * blob: carries the FFmpeg core into its worker, previews and downloads.
 * Vercel Web Analytics loads and reports through /_vercel/insights on this
 * origin, so 'self' covers it.
 */
export function contentSecurityPolicy(isDevelopment: boolean): string {
  return [
    "default-src 'self'",
    [
      "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' blob:",
      BETTER_STACK_SCRIPT_ORIGIN,
      // React's development build evaluates code to rebuild stack traces.
      isDevelopment ? "'unsafe-eval'" : "",
    ]
      .filter(Boolean)
      .join(" "),
    "worker-src 'self' blob:",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "media-src 'self' blob: data:",
    "font-src 'self' data:",
    `connect-src 'self' blob: data: ${GITHUB_API_ORIGIN} ${BETTER_STACK_INGEST_ORIGIN}`,
    "frame-src 'none'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'self'",
    "frame-ancestors 'none'",
  ].join("; ");
}

export function securityHeaders(isDevelopment: boolean) {
  return [
    {
      key: "Content-Security-Policy",
      value: contentSecurityPolicy(isDevelopment),
    },
    { key: "X-Content-Type-Options", value: "nosniff" },
    { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
    // No tool uses these; refusing them means injected script cannot either.
    {
      key: "Permissions-Policy",
      value: "camera=(), microphone=(), geolocation=(), browsing-topics=()",
    },
    // Sent by the app itself, not only by the platform. Browsers ignore it over plain HTTP.
    {
      key: "Strict-Transport-Security",
      value: "max-age=63072000; includeSubDomains; preload",
    },
  ];
}
