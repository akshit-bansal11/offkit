// ToolPill.tsx
// Individual tool link pill with category accent color and in-development state.

import { TriangleAlert } from "lucide-react";
import Link from "next/link";
import { runsOnWeb } from "@/config/tools";
import { cn } from "@/lib/utils";
import type { ToolDefinition } from "@/types/tool";

interface ToolPillProps {
  tool: ToolDefinition;
  isInDevelopment?: boolean;
  accentColor?: string;
}

export function ToolPill({
  tool,
  isInDevelopment = false,
  accentColor,
}: ToolPillProps) {
  const Icon = tool.icon;
  const label = tool.shortName ?? tool.name;

  const pillClass = cn(
    "inline-flex min-h-11 items-center gap-2.5 rounded-full border border-border bg-secondary px-5 py-2.5 text-sm font-medium text-foreground shadow-[inset_1px_1px_0px_rgba(246,247,251,0.06)] outline-none transition-all duration-300 motion-reduce:transition-none",
    isInDevelopment
      ? "cursor-not-allowed opacity-60"
      : "cursor-pointer hover:-translate-y-0.5 hover:bg-secondary/80 hover:border-[var(--pill-accent-border,var(--muted-foreground))] focus-visible:ring-2 focus-visible:ring-ring motion-reduce:hover:translate-y-0",
  );

  const iconStyle = accentColor
    ? { color: accentColor }
    : { color: "var(--muted-foreground)" };

  const content = (
    <>
      {isInDevelopment ? (
        <TriangleAlert className="size-4 shrink-0 text-warning" />
      ) : (
        <Icon
          className="size-4 shrink-0 transition-colors duration-300"
          style={iconStyle}
        />
      )}
      <span>{label}</span>
      {!runsOnWeb(tool) && (
        <span className="rounded-full border border-border px-2 text-xs text-muted-foreground">
          Desktop
        </span>
      )}
    </>
  );

  if (isInDevelopment) {
    return (
      <div
        aria-disabled="true"
        title={`${tool.name} is in development`}
        className={pillClass}
      >
        {content}
      </div>
    );
  }

  return (
    <Link
      href={tool.href}
      className={pillClass}
      style={
        accentColor
          ? ({
              "--pill-accent-border": `${accentColor}59`,
            } as React.CSSProperties) // ~35% alpha
          : undefined
      }
    >
      {content}
    </Link>
  );
}
