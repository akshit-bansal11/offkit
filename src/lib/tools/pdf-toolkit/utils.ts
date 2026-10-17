export function generateId() {
  return Math.random().toString(36).substring(2, 9);
}

export function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export interface PageRangeSelection {
  /** Zero-based page indices, in the order typed. Repeats are kept. */
  pages: number[];
  /** Parts that are not a page or range inside 1..maxPages, as typed. */
  invalid: string[];
}

/**
 * Parse "1-3, 5, 9-7" into page indices in the order typed: a range counts
 * down when its end is below its start, and a page typed twice appears
 * twice. Empty parts (a trailing comma) are skipped; anything else that is
 * not a page number within the document is reported, never dropped quietly.
 */
export function parsePageRanges(
  input: string,
  maxPages: number,
): PageRangeSelection {
  const pages: number[] = [];
  const invalid: string[] = [];
  const inBounds = (page: number) => page >= 1 && page <= maxPages;

  for (const raw of input.split(",")) {
    const part = raw.trim();
    if (!part) continue;

    const match = part.match(/^(\d+)(?:\s*-\s*(\d+))?$/);
    const start = match ? Number(match[1]) : Number.NaN;
    const end = match?.[2] === undefined ? start : Number(match[2]);
    if (!inBounds(start) || !inBounds(end)) {
      invalid.push(part);
      continue;
    }

    const step = end >= start ? 1 : -1;
    for (let page = start; page !== end + step; page += step) {
      pages.push(page - 1);
    }
  }

  return { pages, invalid };
}

/** The message for a selection that cannot be used, or null when it can. */
export function describePageRangeProblem(
  selection: PageRangeSelection,
  maxPages: number,
): string | null {
  if (selection.invalid.length > 0) {
    const parts = selection.invalid.map((part) => `"${part}"`).join(", ");
    return `Not a page or range in this ${maxPages}-page PDF: ${parts}. Use page numbers from 1 to ${maxPages}, like 1-3, 5.`;
  }
  if (selection.pages.length === 0) {
    return "Enter at least one page or range, like 1-3, 5.";
  }
  return null;
}
