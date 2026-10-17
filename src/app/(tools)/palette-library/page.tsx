"use client";

import { CheckCircle2, Copy, Search } from "lucide-react";
import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/design/CopyButton";
import { Input } from "@/components/ui/form/Input";
import { Card, CardHeader, CardTitle } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import { palettePresets } from "@/lib/design-tools/data/palette-presets";

const tool = getToolBySlug("palette-library");

export default function PaletteLibraryPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <PaletteLibraryTool />
    </ToolPageShell>
  );
}

function PaletteLibraryTool() {
  const [search, setSearch] = useState("");
  const [copiedColor, setCopiedColor] = useState<string | null>(null);

  const filteredPalettes = palettePresets.filter((palette) =>
    palette.name.toLowerCase().includes(search.trim().toLowerCase()),
  );

  async function handleCopyColor(color: string) {
    await navigator.clipboard.writeText(color);
    setCopiedColor(color);
    window.setTimeout(() => setCopiedColor(null), 1600);
  }

  const palettesByCount = filteredPalettes.reduce(
    (acc, palette) => {
      const count = palette.colors.length;
      if (!acc[count]) {
        acc[count] = [];
      }
      acc[count].push(palette);
      return acc;
    },
    {} as Record<number, typeof palettePresets>,
  );

  const sortedCounts = Object.keys(palettesByCount)
    .map(Number)
    .sort((a, b) => a - b);

  return (
    <div className="space-y-10">
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
          <div>
            <CardTitle>Curated Color Palettes</CardTitle>
            <p className="mt-2 text-sm text-muted-foreground">
              Click any swatch to copy its hex value or grab the entire palette
              as JSON.
            </p>
          </div>

          <div className="relative w-full lg:max-w-sm xl:max-w-md">
            <Search className="absolute left-4 top-1/2 size-5 -translate-y-1/2 text-muted-foreground/60" />
            <Input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search palettes..."
              className="h-14 rounded-2xl border-white/5 bg-white/[0.03] pl-12 text-lg shadow-inner ring-offset-background placeholder:text-muted-foreground/40 focus:bg-white/[0.05]"
            />
          </div>
        </CardHeader>
      </Card>

      <div className="space-y-16">
        {sortedCounts.map((count) => {
          const palettes = palettesByCount[count];
          if (!palettes || palettes.length === 0) return null;

          return (
            <div key={count} className="space-y-6">
              <h2 className="text-2xl font-bold tracking-tight text-foreground/90">
                {count} Colors
              </h2>
              <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3 xl:gap-8">
                {palettes.map((palette) => (
                  <div
                    key={palette.name}
                    className="group flex flex-col overflow-hidden rounded-[1.5rem] border border-white/5 bg-[#121212] shadow-lg transition-all duration-300 hover:-translate-y-1 hover:border-white/10 hover:shadow-xl hover:shadow-blue-900/10"
                  >
                    {/* Color Bars */}
                    <div className="flex h-32 w-full">
                      {palette.colors.map((color) => (
                        <Button
                          key={color}
                          type="button"
                          variant="ghost"
                          onClick={() => handleCopyColor(color)}
                          aria-label={`Copy ${color.toUpperCase()}`}
                          className="group/color relative h-full flex-1 rounded-none p-0 transition-all duration-300 hover:flex-[1.15] hover:bg-transparent"
                          style={{ backgroundColor: color }}
                        >
                          <div className="absolute inset-0 flex items-center justify-center bg-black/10 opacity-0 transition-opacity duration-300 group-hover/color:opacity-100">
                            {copiedColor === color ? (
                              <CheckCircle2 className="size-8 text-white drop-shadow-md" />
                            ) : (
                              <Copy className="size-8 text-white/60 drop-shadow-md" />
                            )}
                          </div>
                        </Button>
                      ))}
                    </div>

                    {/* Bottom Info Area */}
                    <div className="flex flex-col items-center justify-center p-6 text-center">
                      <h3 className="text-lg font-semibold tracking-wide text-foreground">
                        {palette.name}
                      </h3>
                      <div className="mt-4 flex flex-wrap justify-center gap-x-3 gap-y-2">
                        {palette.colors.map((color) => (
                          <Button
                            key={`${palette.name}-${color}`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => handleCopyColor(color)}
                            className="h-auto px-0 py-0 font-mono text-xs font-medium tracking-wider text-muted-foreground transition-colors hover:bg-transparent hover:text-primary sm:text-[10px] xl:text-xs"
                          >
                            {color.toUpperCase()}
                          </Button>
                        ))}
                      </div>
                      <CopyButton
                        value={JSON.stringify(
                          { name: palette.name, colors: palette.colors },
                          null,
                          2,
                        )}
                        label="Copy JSON"
                        className="mt-5"
                      />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
