// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { contentSecurityPolicy, UNOBSERVED_PATHS } from "./security-headers.ts";
import { tools } from "./tools.ts";

test("every page kept free of Better Stack is a real tool route", () => {
  const hrefs = tools.map((tool) => tool.href);

  assert.deepEqual(
    UNOBSERVED_PATHS.filter((path) => !hrefs.some((href) => href === path)),
    [],
  );
});

test("the production policy allows no eval", () => {
  const policy = contentSecurityPolicy(false);

  assert.doesNotMatch(policy, /'unsafe-eval'/);
});

test("the development policy allows the eval React's dev build needs", () => {
  const policy = contentSecurityPolicy(true);

  assert.match(policy, /script-src [^;]*'unsafe-eval'/);
});

test("the policy lets the GitHub API be the only third-party origin a tool connects to", () => {
  const connect = contentSecurityPolicy(false)
    .split("; ")
    .find((directive) => directive.startsWith("connect-src"));

  assert.equal(
    connect,
    "connect-src 'self' blob: data: https://api.github.com https://s2791328.us-west-2a.betterstackdata.com",
  );
});
