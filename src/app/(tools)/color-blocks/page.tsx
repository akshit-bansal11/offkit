"use client";

import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Card, CardHeader, CardTitle } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import { colorBlocks } from "@/lib/design-tools/data/color-blocks";
import {
  type HueFamily,
  hueFamilies,
  hueFamily,
  type Lightness,
  lightness,
} from "@/lib/design-tools/palette-tools";

const tool = getToolBySlug("color-blocks");

const lightnessOptions: readonly Lightness[] = ["light", "dark"];

const blocks = colorBlocks.map((hex) => ({
  hex,
  family: hueFamily(hex),
  lightness: lightness(hex),
}));

export default function ColorBlocksPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <ColorBlocksTool />
    </ToolPageShell>
  );
}

function ColorBlocksTool() {
  const [family, setFamily] = useState<HueFamily | null>(null);
  const [tone, setTone] = useState<Lightness | null>(null);
  const [status, setStatus] = useState("");

  const visible = blocks.filter(
    (block) =>
      (family === null || block.family === family) &&
      (tone === null || block.lightness === tone),
  );

  async function handleCopy(hex: string) {
    try {
      await navigator.clipboard.writeText(hex);
      setStatus(`Copied ${hex}`);
    } catch {
      setStatus(`Could not copy ${hex}`);
    }
  }

  return (
    <div className="space-y-8">
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="space-y-5">
          <CardTitle>Color Blocks</CardTitle>
          <fieldset className="flex flex-wrap items-center gap-2">
            <legend className="mb-2 text-sm text-muted-foreground">
              Hue family
            </legend>
            <FilterButton active={family === null} onClick={() => setFamily(null)}>
              All
            </FilterButton>
            {hueFamilies.map((name) => (
              <FilterButton
                key={name}
                active={family === name}
                onClick={() => setFamily(family === name ? null : name)}
              >
                {name}
              </FilterButton>
            ))}
          </fieldset>
          <fieldset className="flex flex-wrap items-center gap-2">
            <legend className="mb-2 text-sm text-muted-foreground">
              Lightness
            </legend>
            <FilterButton active={tone === null} onClick={() => setTone(null)}>
              All
            </FilterButton>
            {lightnessOptions.map((name) => (
              <FilterButton
                key={name}
                active={tone === name}
                onClick={() => setTone(tone === name ? null : name)}
              >
                {name}
              </FilterButton>
            ))}
          </fieldset>
          <p className="text-sm text-muted-foreground">
            {visible.length} of {blocks.length} colors. Click a block to copy
            its hex.
          </p>
        </CardHeader>
      </Card>

      <p role="status" className="sr-only">
        {status}
      </p>

      {visible.length === 0 ? (
        <p className="rounded-2xl border border-white/10 bg-background/40 p-8 text-center text-sm text-muted-foreground">
          No colors match these filters.
        </p>
      ) : (
        <ul className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
          {visible.map((block) => (
            <li key={block.hex}>
              <button
                type="button"
                onClick={() => handleCopy(block.hex)}
                aria-label={`Copy ${block.hex}`}
                className={`flex h-24 w-full items-end rounded-xl border border-white/10 p-2 font-mono text-xs outline-none transition-transform hover:scale-[1.03] focus-visible:ring-[3px] focus-visible:ring-ring motion-reduce:transition-none motion-reduce:hover:scale-100 ${
                  block.lightness === "light" ? "text-black" : "text-white"
                }`}
                style={{ backgroundColor: block.hex }}
              >
                {block.hex}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function FilterButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: string;
}) {
  return (
    <Button
      type="button"
      size="sm"
      variant={active ? "default" : "outline"}
      aria-pressed={active}
      onClick={onClick}
      className="capitalize"
    >
      {children}
    </Button>
  );
}
