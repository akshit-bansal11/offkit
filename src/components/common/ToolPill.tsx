// ToolPill.tsx
// Individual tool link pill with category accent color and in-development state.

import { TriangleAlert } from "lucide-react";
import Link from "next/link";
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
    "inline-flex min-h-11 items-center gap-2.5 rounded-full border border-white/[0.08] bg-[#252626] px-5 py-2.5 text-sm font-medium text-[#e7e5e4] shadow-[inset_1px_1px_0px_rgba(72,72,72,0.15)] transition-all duration-300",
    isInDevelopment
      ? "cursor-not-allowed opacity-60"
      : "hover:bg-[#2b2c2c] hover:border-[var(--pill-accent-border,rgba(255,255,255,0.25))]",
  );

  const iconStyle = accentColor
    ? { color: `${accentColor}99` }
    : { color: "#acabaa" };

  const content = (
    <>
      {isInDevelopment ? (
        <TriangleAlert className="size-4 shrink-0 text-amber-300" />
      ) : (
        <Icon
          className="size-4 shrink-0 transition-colors duration-300"
          style={iconStyle}
        />
      )}
      <span>{label}</span>
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
