// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { buildTree, parseRepoIdentity, treeToAscii } from "./utils.ts";

test("treeToAscii draws the tree with box-drawing characters", () => {
  const root = buildTree([
    { path: "src", type: "tree", sha: "a" },
    { path: "src/a.ts", type: "blob", sha: "b" },
    { path: "src/b.ts", type: "blob", sha: "c" },
    { path: "README.md", type: "blob", sha: "d" },
  ]);

  // The glyphs were once saved double-encoded, so the copy read "â”œâ”€â”€".
  assert.equal(
    treeToAscii(root),
    [".", "├── src/", "│   ├── a.ts", "│   └── b.ts", "└── README.md"].join(
      "\n",
    ),
  );
});

test("parseRepoIdentity reads a GitHub URL with a path, query and .git suffix", () => {
  const identity = parseRepoIdentity(
    "https://github.com/vercel/next.js.git/tree/canary?tab=readme",
  );

  assert.deepEqual(identity, { owner: "vercel", repo: "next.js" });
});

test("parseRepoIdentity reads the owner/repo short form", () => {
  const identity = parseRepoIdentity(" octo-org/my_repo ");

  assert.deepEqual(identity, { owner: "octo-org", repo: "my_repo" });
});

test("parseRepoIdentity refuses names that could change the API path or query", () => {
  const attempts = ["a/b?x=1", "a/b#c", "a/..", "a/%2e%2e", "-a/b", "a b/c"];

  const parsed = attempts.map(parseRepoIdentity);

  assert.deepEqual(parsed, [null, null, null, null, null, null]);
});

test("parseRepoIdentity refuses a URL on another host", () => {
  const identity = parseRepoIdentity("https://gitlab.com/owner/repo");

  assert.equal(identity, null);
});
