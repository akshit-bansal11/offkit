// Runs under Node's built-in test runner: `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { describePageRangeProblem, parsePageRanges } from "./utils.ts";

// The previous parser returned a sorted, de-duplicated set and silently dropped
// any part it could not use, so "5, 1-2" gave pages 1, 2, 5 and "9" on a
// 5-page file gave nothing at all with no reason. Both were bugs.

test("parsePageRanges keeps pages in the order typed", () => {
  const selection = parsePageRanges("5, 1-2", 6);

  assert.deepEqual(selection, { pages: [4, 0, 1], invalid: [] });
});

test("parsePageRanges keeps a page typed twice", () => {
  const selection = parsePageRanges("2, 2", 3);

  assert.deepEqual(selection.pages, [1, 1]);
});

test("parsePageRanges counts a descending range down", () => {
  const selection = parsePageRanges("4-2", 5);

  assert.deepEqual(selection.pages, [3, 2, 1]);
});

test("parsePageRanges reports out-of-bounds and malformed parts as typed", () => {
  const selection = parsePageRanges("0, 1, 9, 2-99, 3a, 1-2-3", 5);

  assert.deepEqual(selection, {
    pages: [0],
    invalid: ["0", "9", "2-99", "3a", "1-2-3"],
  });
});

test("parsePageRanges skips empty parts such as a trailing comma", () => {
  const selection = parsePageRanges(" 1 - 2 , ", 5);

  assert.deepEqual(selection, { pages: [0, 1], invalid: [] });
});

test("describePageRangeProblem names every invalid part and the page count", () => {
  const message = describePageRangeProblem(parsePageRanges("1, 9, x", 5), 5);

  assert.equal(
    message,
    'Not a page or range in this 5-page PDF: "9", "x". Use page numbers from 1 to 5, like 1-3, 5.',
  );
});

test("describePageRangeProblem asks for a page when nothing was typed", () => {
  const message = describePageRangeProblem(parsePageRanges("  ", 5), 5);

  assert.equal(message, "Enter at least one page or range, like 1-3, 5.");
});

test("describePageRangeProblem accepts a usable selection", () => {
  const message = describePageRangeProblem(parsePageRanges("1-3", 5), 5);

  assert.equal(message, null);
});
