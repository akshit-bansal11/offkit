// main.mjs
// Electron entry point: serves the static export in out/ over app:// and filters what the window may load or open.

import { existsSync, statSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { app, BrowserWindow, net, protocol, session, shell } from "electron";

// `new URL("app://…").origin` is "null" for a non-special scheme, so origins are compared as string prefixes.
const ORIGIN = "app://open-tools";
const ROOT = path.join(app.getAppPath(), "out");

// Schemes a page may load from. file: is deliberately absent: only this process reads the export from disk.
const PAGE_SCHEMES = new Set(["app:", "blob:", "data:", "devtools:"]);
// Git Scaffold reads a repository's file list from GitHub. It is the only tool allowed online,
// and only for that one read: allowing all of api.github.com would let injected script post data out.
const GITHUB_API = "https://api.github.com";
// /repositories/<id>/… is where GitHub redirects a renamed or transferred repository.
const GITHUB_TREE_PATH =
  /^\/(repos\/[^/]+\/[^/]+|repositories\/\d+)\/git\/trees\//;
// OPTIONS is the preflight the browser sends before a request that carries a token.
const GITHUB_METHODS = new Set(["GET", "OPTIONS"]);
// The app links to its own repository and nowhere else. Opening arbitrary URLs would let
// injected script hand data to the default browser, which this process cannot filter.
const EXTERNAL_LINK_PREFIX = "https://github.com/";

// A static export inlines its bootstrap scripts, so a nonce-based policy is not possible here.
// The request filter in lockDown() is the main control on what a page can load; this narrows the rest.
const CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' blob:",
  "worker-src 'self' blob:",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob: data:",
  "media-src 'self' blob: data:",
  "font-src 'self' data:",
  `connect-src 'self' blob: data: ${GITHUB_API}`,
  "frame-src 'self' blob:",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

protocol.registerSchemesAsPrivileged([
  {
    scheme: "app",
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      stream: true,
      codeCache: true,
    },
  },
]);

/** Maps a URL path to a file inside the export, or null when there is none. */
function resolveFile(pathname) {
  const requested = path.join(ROOT, decodeURIComponent(pathname));
  // Refuse anything that climbs out of the export directory.
  if (requested !== ROOT && !requested.startsWith(ROOT + path.sep)) return null;

  const candidates = [
    requested,
    `${requested}.html`,
    path.join(requested, "index.html"),
  ];
  return (
    candidates.find(
      (candidate) => existsSync(candidate) && statSync(candidate).isFile(),
    ) ?? null
  );
}

async function serve(request) {
  const file = resolveFile(new URL(request.url).pathname);
  const response = await net.fetch(
    pathToFileURL(file ?? path.join(ROOT, "404.html")).toString(),
  );
  const headers = new Headers(response.headers);
  headers.set("Content-Security-Policy", CSP);
  return new Response(response.body, { status: file ? 200 : 404, headers });
}

/** Copy buttons exist on most tools; only the Diff Checker pastes, so only it may read the clipboard. */
function isAllowedPermission(permission, requestingUrl) {
  if (!requestingUrl?.startsWith(`${ORIGIN}/`)) return false;
  if (permission === "clipboard-sanitized-write") return true;
  return (
    permission === "clipboard-read" &&
    requestingUrl.startsWith(`${ORIGIN}/diff-checker`)
  );
}

function isAllowedRequest(details) {
  const url = new URL(details.url);
  // This process's own reads of the export carry no webContents; a page asking for file: is refused.
  if (url.protocol === "file:") return !details.webContents;
  if (PAGE_SCHEMES.has(url.protocol)) return true;
  return (
    GITHUB_METHODS.has(details.method) &&
    url.origin === GITHUB_API &&
    GITHUB_TREE_PATH.test(url.pathname)
  );
}

function lockDown() {
  session.defaultSession.webRequest.onBeforeRequest((details, callback) => {
    callback({ cancel: !isAllowedRequest(details) });
  });
  session.defaultSession.setPermissionRequestHandler(
    (_contents, permission, callback, details) =>
      callback(isAllowedPermission(permission, details.requestingUrl)),
  );
  session.defaultSession.setPermissionCheckHandler(
    (_contents, permission, _origin, details) =>
      isAllowedPermission(permission, details.requestingUrl),
  );
}

function openInBrowser(url) {
  if (url.startsWith(EXTERNAL_LINK_PREFIX)) shell.openExternal(url);
}

function createWindow() {
  const window = new BrowserWindow({
    width: 1280,
    height: 860,
    // Matches .page-shell in globals.css, so the window does not flash white before first paint.
    backgroundColor: "#080808",
    autoHideMenuBar: true,
    webPreferences: {
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  // WebRTC traffic does not go through the request filter, and no tool uses it.
  window.webContents.setWebRTCIPHandlingPolicy("disable_non_proxied_udp");

  // Links that leave the app open in the default browser; the window itself never navigates away.
  window.webContents.setWindowOpenHandler(({ url }) => {
    openInBrowser(url);
    return { action: "deny" };
  });
  window.webContents.on("will-navigate", (event, url) => {
    if (url.startsWith(`${ORIGIN}/`)) return;
    event.preventDefault();
    openInBrowser(url);
  });

  window.loadURL(`${ORIGIN}/`);
}

app.whenReady().then(() => {
  protocol.handle("app", serve);
  lockDown();
  createWindow();
});

app.on("window-all-closed", () => app.quit());
