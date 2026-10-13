// main.mjs
// Electron entry point: serves the static export in out/ over app:// and keeps the window off the network.

import { existsSync, statSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { app, BrowserWindow, net, protocol, session, shell } from "electron";

// `new URL("app://…").origin` is "null" for a non-special scheme, so origins are compared as string prefixes.
const ORIGIN = "app://open-tools";
const ROOT = path.join(app.getAppPath(), "out");

const LOCAL_SCHEMES = new Set(["app:", "file:", "blob:", "data:", "devtools:"]);
// Git Scaffold reads a repository's file list from GitHub. It is the only tool allowed online.
const ALLOWED_REMOTE_ORIGINS = ["https://api.github.com"];
// Copy buttons, and the Diff Checker's paste buttons. Every other permission is refused.
const ALLOWED_PERMISSIONS = new Set([
  "clipboard-sanitized-write",
  "clipboard-read",
]);

// A static export inlines its bootstrap scripts, so a nonce-based policy is not possible here.
// The request filter in lockDown() is what actually keeps the app offline; this narrows the rest.
const CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' blob:",
  "worker-src 'self' blob:",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob: data:",
  "media-src 'self' blob: data:",
  "font-src 'self' data:",
  `connect-src 'self' blob: data: ${ALLOWED_REMOTE_ORIGINS.join(" ")}`,
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

function lockDown() {
  session.defaultSession.webRequest.onBeforeRequest((details, callback) => {
    const url = new URL(details.url);
    callback({
      cancel:
        !LOCAL_SCHEMES.has(url.protocol) &&
        !ALLOWED_REMOTE_ORIGINS.includes(url.origin),
    });
  });
  session.defaultSession.setPermissionRequestHandler(
    (_contents, permission, callback) =>
      callback(ALLOWED_PERMISSIONS.has(permission)),
  );
  session.defaultSession.setPermissionCheckHandler((_contents, permission) =>
    ALLOWED_PERMISSIONS.has(permission),
  );
}

function openInBrowser(url) {
  if (url.startsWith("https://")) shell.openExternal(url);
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
