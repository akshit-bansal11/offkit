// github.ts
// Reads a repository's file tree from the GitHub REST API: the one network call any tool makes.

import type { GitTreeEntry, RepoIdentity } from "./types";

// The only origin a token is ever sent to. The path is built from a validated owner and repo,
// so no input can point the request, and the token with it, anywhere else.
const GITHUB_API = "https://api.github.com";

export interface RepoTree {
  entries: GitTreeEntry[];
  /** GitHub stops listing very large trees; the entries are then a partial list. */
  truncated: boolean;
}

export function treeUrl({ owner, repo }: RepoIdentity): string {
  // The desktop app's request filter accepts exactly this path and query.
  return `${GITHUB_API}/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/git/trees/HEAD?recursive=1`;
}

/**
 * Fetch the full tree of a repository's default branch. A token, when given,
 * travels only in the Authorization header: never in the URL, so it cannot
 * end up in history, logs or a Referer. Throws an Error whose message is
 * ready to show to the user.
 */
export async function fetchRepoTree(
  repo: RepoIdentity,
  token: string,
): Promise<RepoTree> {
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
  };
  if (token) headers.Authorization = `Bearer ${token}`;

  let response: Response;
  try {
    response = await fetch(treeUrl(repo), { headers });
  } catch {
    throw new Error(
      "Unable to reach GitHub. Check your network connection and try again.",
    );
  }

  if (!response.ok) {
    throw new Error(
      describeTreeFailure(
        response.status,
        response.headers.get("x-ratelimit-remaining"),
        token !== "",
      ),
    );
  }

  const tree = parseTreeResponse(await response.json());
  if (!tree) {
    throw new Error("GitHub sent a response this tool could not read.");
  }
  return tree;
}

export function describeTreeFailure(
  status: number,
  rateLimitRemaining: string | null,
  hasToken: boolean,
): string {
  const rateLimited =
    status === 429 || (status === 403 && rateLimitRemaining === "0");
  if (rateLimited) {
    return hasToken
      ? "GitHub's API rate limit for this token is used up. Try again later."
      : "GitHub's API rate limit for requests without a token is used up. Add a personal access token, or try again later.";
  }
  if (status === 401) {
    return "GitHub rejected the token (401). Check that it is correct and has not expired.";
  }
  if (status === 403) {
    return hasToken
      ? "GitHub refused the request (403). The token may not have access to this repository."
      : "GitHub refused the request (403).";
  }
  if (status === 404) {
    // GitHub answers 404, not 403, for a private repository the caller cannot see.
    return hasToken
      ? "Repository not found, or the token cannot read it. Check the name and the token's repository access."
      : "Repository not found. If it is private, turn on Private repository and add a token that can read it.";
  }
  if (status === 409) {
    return "This repository is empty, so there is no tree to show.";
  }
  return `GitHub request failed with status ${status}.`;
}

function isTreeEntry(value: unknown): value is GitTreeEntry {
  return (
    typeof value === "object" &&
    value !== null &&
    "path" in value &&
    typeof value.path === "string" &&
    "sha" in value &&
    typeof value.sha === "string" &&
    "type" in value &&
    (value.type === "blob" || value.type === "tree")
  );
}

/**
 * Keep files and folders from GitHub's tree response; submodules ("commit"
 * entries) are dropped. Returns null when the payload is not a tree response.
 */
export function parseTreeResponse(payload: unknown): RepoTree | null {
  if (
    typeof payload !== "object" ||
    payload === null ||
    !("tree" in payload) ||
    !Array.isArray(payload.tree)
  ) {
    return null;
  }

  return {
    entries: payload.tree.filter(isTreeEntry),
    truncated: "truncated" in payload && payload.truncated === true,
  };
}
