// CategoryCard.tsx
// Renders a category section with accent-colored header and a grid of tool pills.

import { ToolPill } from "@/components/common/ToolPill";
import type { ToolCategoryDefinition, ToolDefinition } from "@/types/tool";

interface CategoryCardProps {
  category: ToolCategoryDefinition;
  tools: ToolDefinition[];
  inDevelopmentSlugs?: Set<string>;
  headingLevel?: "h2" | "h3";
}

export function CategoryCard({
  category,
  tools,
  inDevelopmentSlugs,
  headingLevel = "h2",
}: CategoryCardProps) {
  const Heading = headingLevel;
  const CategoryIcon = category.icon;
  const accent = category.accentColor ?? "#ffffff";

  return (
    <section className="h-full">
      <div
        className="group h-full overflow-hidden rounded-3xl border border-white/10 bg-[#0f0f0f]/72 shadow-[0_0_0_1px_rgba(255,255,255,0.02)] backdrop-blur-xl transition-all duration-300"
        style={
          {
            "--accent": accent,
            "--accent-border": `${accent}66`,
            "--accent-bg": `${accent}24`,
            "--accent-shadow": `${accent}48`,
          } as React.CSSProperties
        }
      >
        <div
          className="h-[2px] w-full opacity-70 transition-opacity duration-300 group-hover:opacity-100"
          style={{
            background: `linear-gradient(90deg, transparent, ${accent}88, transparent)`,
          }}
        />

        <div className="border-b border-white/8 bg-[linear-gradient(180deg,rgba(44,44,44,0.52)_0%,rgba(31,31,31,0.38)_100%)] p-2">
          <div className="flex justify-between items-center relative rounded-2xl border border-white/8 bg-white/[0.015] lg:p-4 md:p-3 sm:p-2 p-2">
            <Heading className="lg:text-4xl md:text-3xl sm:text-2xl text-xl font-medium tracking-[-0.04em] text-foreground">
              {category.name}
            </Heading>

            <div
              className="w-min rounded-2xl border p-3 transition-all duration-300
                group-hover:border-[var(--accent-border)]
                group-hover:bg-[var(--accent-bg)]
                group-hover:shadow-[0_0_22px_var(--accent-shadow),0_0_8px_var(--accent-shadow)]"
              style={{
                borderColor: `${accent}28`,
                backgroundColor: `${accent}0d`,
              }}
            >
              <span
                className="block transition-all duration-300"
                style={{ color: `${accent}cc` }}
              >
                <CategoryIcon
                  className="lg:size-9 md:size-7 sm:size-5 size-5"
                  strokeWidth={1.75}
                />
              </span>
            </div>
          </div>
        </div>

        <div className="bg-white/[0.02] backdrop-blur-md px-6 py-6 sm:px-7 sm:py-7">
          <div className="mb-4 flex items-center justify-between border-b border-white/8 pb-3 text-xs uppercase tracking-[0.16em] text-white/40">
            <span
              className="border-l-2 pl-2 transition-colors duration-300"
              style={{ borderColor: `${accent}66` }}
            >
              Tools
            </span>
          </div>

          <div className="grid grid-cols-1 content-start items-start gap-3 sm:grid-cols-2">
            {tools.map((tool) => (
              <ToolPill
                key={tool.slug}
                tool={tool}
                isInDevelopment={inDevelopmentSlugs?.has(tool.slug)}
                accentColor={accent}
              />
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
