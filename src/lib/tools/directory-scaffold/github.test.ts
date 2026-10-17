// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";

import {
  describeTreeFailure,
  fetchRepoTree,
  parseTreeResponse,
} from "./github.ts";

const REPO = { owner: "octo-org", repo: "private-repo" };

function treeResponse() {
  return new Response(JSON.stringify({ tree: [], truncated: false }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

afterEach(() => mock.restoreAll());

test("fetchRepoTree sends the token only in the Authorization header to api.github.com", async () => {
  const fetchMock = mock.method(globalThis, "fetch", async () =>
    treeResponse(),
  );

  await fetchRepoTree(REPO, "ghp_secret");

  const [url, init] = fetchMock.mock.calls[0].arguments;
  assert.deepEqual(
    { url: String(url), headers: init?.headers },
    {
      url: "https://api.github.com/repos/octo-org/private-repo/git/trees/HEAD?recursive=1",
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: "Bearer ghp_secret",
      },
    },
  );
});

test("fetchRepoTree sends no Authorization header without a token", async () => {
  const fetchMock = mock.method(globalThis, "fetch", async () =>
    treeResponse(),
  );

  await fetchRepoTree(REPO, "");

  assert.deepEqual(fetchMock.mock.calls[0].arguments[1]?.headers, {
    Accept: "application/vnd.github+json",
  });
});

test("fetchRepoTree turns a 404 without a token into the private-repository hint", async () => {
  mock.method(
    globalThis,
    "fetch",
    async () => new Response("{}", { status: 404 }),
  );

  await assert.rejects(fetchRepoTree(REPO, ""), /If it is private/);
});

test("fetchRepoTree reports a network failure in its own words", async () => {
  mock.method(globalThis, "fetch", async () => {
    throw new TypeError("Failed to fetch");
  });

  await assert.rejects(fetchRepoTree(REPO, "ghp_secret"), {
    message:
      "Unable to reach GitHub. Check your network connection and try again.",
  });
});

test("describeTreeFailure recognises the rate limit from the remaining-requests header", () => {
  const message = describeTreeFailure(403, "0", false);

  assert.match(message, /rate limit/);
});

test("describeTreeFailure treats any other 403 as a refusal, not a rate limit", () => {
  const message = describeTreeFailure(403, "42", true);

  assert.equal(
    message,
    "GitHub refused the request (403). The token may not have access to this repository.",
  );
});

test("describeTreeFailure treats 429 as the rate limit", () => {
  const message = describeTreeFailure(429, null, true);

  assert.match(message, /rate limit/);
});

test("describeTreeFailure explains a rejected token", () => {
  const message = describeTreeFailure(401, null, true);

  assert.match(message, /rejected the token/);
});

test("describeTreeFailure points a 404 with a token at the token's access", () => {
  const message = describeTreeFailure(404, null, true);

  assert.match(message, /token cannot read it/);
});

test("describeTreeFailure explains an empty repository", () => {
  const message = describeTreeFailure(409, null, false);

  assert.match(message, /empty/);
});

test("describeTreeFailure falls back to the status code", () => {
  const message = describeTreeFailure(500, null, false);

  assert.equal(message, "GitHub request failed with status 500.");
});

test("parseTreeResponse keeps files and folders and drops submodules", () => {
  const tree = parseTreeResponse({
    tree: [
      { path: "a", type: "tree", sha: "1" },
      { path: "a/b.ts", type: "blob", sha: "2" },
      { path: "vendor/lib", type: "commit", sha: "3" },
    ],
    truncated: true,
  });

  assert.deepEqual(tree, {
    entries: [
      { path: "a", type: "tree", sha: "1" },
      { path: "a/b.ts", type: "blob", sha: "2" },
    ],
    truncated: true,
  });
});

test("parseTreeResponse rejects a payload that is not a tree", () => {
  const tree = parseTreeResponse({ message: "Not Found" });

  assert.equal(tree, null);
});
