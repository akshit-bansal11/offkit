/**
 * Patch all tool pages to pass accentColor to ToolPageShell.
 * Run: node scripts/patch-tool-accents.mjs
 */
import { readdirSync, readFileSync, statSync, writeFileSync } from "fs";
import { join } from "path";

const TOOLS_DIR = join(process.cwd(), "src/app/(tools)");
const dirs = readdirSync(TOOLS_DIR).filter((d) =>
  statSync(join(TOOLS_DIR, d)).isDirectory(),
);

let patched = 0;
let skipped = 0;

for (const dir of dirs) {
  const filePath = join(TOOLS_DIR, dir, "page.tsx");
  let src;
  try {
    src = readFileSync(filePath, "utf8");
  } catch {
    console.log(`  SKIP (no page.tsx): ${dir}`);
    skipped++;
    continue;
  }

  // Already patched?
  if (src.includes("getToolAccentColor") || src.includes("accentColor={")) {
    console.log(`  ALREADY DONE: ${dir}`);
    skipped++;
    continue;
  }

  // Find the slug used in getToolBySlug
  const slugMatch = src.match(/getToolBySlug\("([^"]+)"\)/);
  if (!slugMatch) {
    console.log(`  SKIP (no slug found): ${dir}`);
    skipped++;
    continue;
  }
  const slug = slugMatch[1];

  // 1. Add getToolAccentColor to the import from @/config/tools
  let next = src.replace(
    /import \{([^}]*getToolBySlug[^}]*)\} from "@\/config\/tools"/,
    (match, inner) => {
      if (inner.includes("getToolAccentColor")) return match;
      return `import {${inner}, getToolAccentColor } from "@/config/tools"`;
    },
  );

  // 2. Inject accentColor prop into <ToolPageShell ...>
  //    Matches: <ToolPageShell title={...} description={...}>
  next = next.replace(
    /(<ToolPageShell\s+title=\{[^}]+\}\s+description=\{[^}]+\})>/,
    `$1\n      accentColor={getToolAccentColor("${slug}")}>`,
  );

  if (next === src) {
    console.log(`  SKIP (regex didn't match): ${dir}`);
    skipped++;
    continue;
  }

  writeFileSync(filePath, next, "utf8");
  console.log(`  PATCHED: ${dir} (slug="${slug}")`);
  patched++;
}

console.log(`\nDone — patched: ${patched}, skipped: ${skipped}`);
