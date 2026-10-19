"use client";

import { Download, Search } from "lucide-react";
import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/design/CopyButton";
import { Badge } from "@/components/ui/feedback/Badge";
import { Input } from "@/components/ui/form/Input";
import { getToolBySlug } from "@/config/tools";
import { gradientPresets } from "@/lib/design-tools/data/gradient-presets";
import {
  downloadGradientPng,
  gradientToBackgroundProperty,
  gradientToTailwindArbitrary,
} from "@/lib/design-tools/gradients";
import { cn } from "@/lib/utils";

const tool = getToolBySlug("gradient-library");

export default function GradientLibraryPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <GradientLibraryTool />
    </ToolPageShell>
  );
}

const FEELS = [
  "All",
  ...new Set(gradientPresets.map((gradient) => gradient.feel)),
];

function GradientLibraryTool() {
  const [search, setSearch] = useState("");
  const [feel, setFeel] = useState("All");
  const [downloadingName, setDownloadingName] = useState<string | null>(null);

  const filteredPresets = gradientPresets.filter((gradient) => {
    const matchesSearch = gradient.name
      .toLowerCase()
      .includes(search.trim().toLowerCase());
    const matchesFeel = feel === "All" || gradient.feel === feel;

    return matchesSearch && matchesFeel;
  });

  async function handleDownload(name: string, css: string) {
    try {
      setDownloadingName(name);
      await downloadGradientPng(
        css,
        `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}.png`,
      );
    } finally {
      setDownloadingName(null);
    }
  }

  return (
    <div className="space-y-10">
      {/* Search and Filters Header */}
      <div className="flex flex-col gap-6 lg:flex-row lg:items-center lg:justify-between">
        <div className="relative w-full shrink-0 lg:max-w-sm xl:max-w-md">
          <Search className="absolute left-4 top-1/2 size-5 -translate-y-1/2 text-muted-foreground/50" />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search gradients..."
            className="h-14 w-full rounded-2xl border-white/10 bg-background/50 pl-12 text-base shadow-inner ring-offset-background transition-colors placeholder:text-muted-foreground/50 focus:border-primary focus:bg-background"
          />
        </div>
        <div className="flex flex-1 flex-wrap items-center gap-3 lg:justify-end">
          <span className="text-sm font-medium text-muted-foreground/60">
            Feel:
          </span>
          <div className="flex flex-wrap gap-2">
            {FEELS.map((feelOption) => (
              <Button
                key={feelOption}
                type="button"
                onClick={() => setFeel(feelOption)}
                variant={feel === feelOption ? "default" : "outline"}
                size="sm"
                className={cn(
                  "rounded-full px-4 py-2 text-sm font-medium transition-all duration-200",
                  feel === feelOption
                    ? "btn-accent shadow-md"
                    : "border-white/10 bg-white/[0.03] text-muted-foreground hover:border-white/20 hover:bg-white/[0.08] hover:text-foreground",
                )}
              >
                {feelOption}
              </Button>
            ))}
          </div>
        </div>
      </div>

      {/* Gradient Grid */}
      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3 xl:gap-8">
        {filteredPresets.map((gradient) => {
          const css = gradientToBackgroundProperty(gradient.css);
          const tailwind = gradientToTailwindArbitrary(gradient.css);

          return (
            <div
              key={gradient.name}
              className="group flex flex-col overflow-hidden rounded-[1.5rem] border border-white/5 bg-card/50 shadow-lg transition-all duration-300 hover:-translate-y-1 hover:border-white/10 hover:shadow-xl hover:shadow-info/10"
            >
              {/* Gradient Preview */}
              <div className="relative h-32 w-full shrink-0 overflow-hidden md:h-36">
                <div
                  className="absolute inset-0 h-full w-full transition-transform duration-700 ease-out group-hover:scale-105"
                  style={{ backgroundImage: gradient.css }}
                />
              </div>

              {/* Bottom Info Area */}
              <div className="flex flex-1 flex-col bg-card/70 p-6 z-10">
                <div className="flex items-start justify-between gap-3 mb-5">
                  <h3 className="text-xl font-bold tracking-tight text-foreground truncate">
                    {gradient.name}
                  </h3>
                  <Badge
                    variant="secondary"
                    className="shrink-0 bg-white/5 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground ring-1 ring-white/10"
                  >
                    {gradient.feel}
                  </Badge>
                </div>

                <div className="mt-auto flex items-center gap-2">
                  <CopyButton
                    value={tailwind}
                    label="Tailwind"
                    variant="secondary"
                    className="h-10 flex-1 rounded-xl border-white/5 bg-white/5 text-xs font-medium text-muted-foreground transition-all hover:bg-white/10 hover:text-foreground active:scale-95"
                  />
                  <CopyButton
                    value={css}
                    label="CSS"
                    variant="secondary"
                    className="h-10 flex-1 rounded-xl border-white/5 bg-white/5 text-xs font-medium text-muted-foreground transition-all hover:bg-white/10 hover:text-foreground active:scale-95"
                  />
                  <Button
                    type="button"
                    onClick={() => handleDownload(gradient.name, gradient.css)}
                    disabled={downloadingName === gradient.name}
                    variant="default"
                    className="h-10 w-10 shrink-0 rounded-xl p-0 shadow-lg active:scale-95 btn-accent"
                  >
                    <Download className="size-4 text-white" />
                  </Button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
