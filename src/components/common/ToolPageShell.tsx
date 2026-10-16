"use client";

// ToolPageShell.tsx
// Shared page wrapper for all tool routes with back button, title, and footer.

import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import type { CSSProperties, ReactNode } from "react";
import { useMemo } from "react";
import { RepositoryCorner } from "@/components/common/RepositoryCorner";
import { Button } from "@/components/ui/Button";
import { getToolBySlug, toolCategories } from "@/config/tools";

interface ToolPageShellProps {
  title: string;
  description: string;
  children: ReactNode;
  accentColor?: string;
}

export function ToolPageShell({
  title,
  description,
  children,
}: ToolPageShellProps) {
  const pathname = usePathname();

  const theme = useMemo(() => {
    const slug = pathname.split("/").filter(Boolean)[0] ?? "";
    const tool = getToolBySlug(slug);
    const category = tool?.category;
    const primary =
      toolCategories.find((entry) => entry.name === category)?.accentColor ??
      "#60a5fa";

    const secondary = primary;

    const toRgbTriplet = (hex: string): string => {
      const normalized = hex.replace("#", "");
      const full =
        normalized.length === 3
          ? normalized
              .split("")
              .map((char) => char + char)
              .join("")
          : normalized;
      const int = Number.parseInt(full, 16);
      const r = (int >> 16) & 255;
      const g = (int >> 8) & 255;
      const b = int & 255;
      return `${r} ${g} ${b}`;
    };

    return {
      primary,
      secondary,
      primaryRgb: toRgbTriplet(primary),
      secondaryRgb: toRgbTriplet(secondary),
    };
  }, [pathname]);

  const scopeStyle = {
    "--tool-accent-primary": theme.primary,
    "--tool-accent-secondary": theme.secondary,
    "--tool-accent-primary-rgb": theme.primaryRgb,
    "--tool-accent-secondary-rgb": theme.secondaryRgb,
  } as CSSProperties;

  return (
    <div className="page-shell tool-accent-scope" style={scopeStyle}>
      <div className="page-grid-overlay" />
      <div
        aria-hidden="true"
        className="pointer-events-none fixed inset-0 overflow-hidden z-0"
      >
        <div
          className="absolute -top-28 -left-16 h-[420px] w-[420px] rounded-full blur-[110px]"
          style={{ backgroundColor: `rgb(${theme.primaryRgb} / 0.14)` }}
        />
        <div
          className="absolute top-16 right-0 h-[320px] w-[320px] rounded-full blur-[95px]"
          style={{ backgroundColor: `rgb(${theme.secondaryRgb} / 0.12)` }}
        />
        <div
          className="absolute bottom-8 left-1/2 -translate-x-1/2 h-[220px] w-[540px] rounded-full blur-[120px]"
          style={{
            background: `linear-gradient(90deg, rgb(${theme.primaryRgb} / 0.1), rgb(${theme.secondaryRgb} / 0.1))`,
          }}
        />
      </div>

      <RepositoryCorner className="fixed right-0 top-0 z-20 p-2 sm:p-3" />

      <main className="page-main relative z-10">
        <div className="mb-4 flex justify-start">
          <Button asChild variant="outline" size="sm" className="glass-button">
            <Link href="/">
              <ArrowLeft className="size-4" />
              All tools
            </Link>
          </Button>
        </div>

        <header className="mb-8 text-center">
          <h1
            className="text-4xl font-semibold tracking-tight break-words sm:text-8xl"
            style={{
              background: `linear-gradient(135deg, rgb(${theme.primaryRgb} / 0.5) 0%, rgb(${theme.primaryRgb} / 1) 45%, rgb(${theme.primaryRgb} / 0.65) 100%)`,
              WebkitBackgroundClip: "text",
              WebkitTextFillColor: "transparent",
              backgroundClip: "text",
            }}
          >
            {title}
          </h1>
          {description && (
            <p className="mx-auto mt-4 max-w-2xl text-base leading-7 text-muted-foreground sm:text-lg">
              {description}
            </p>
          )}
        </header>
        {children}
      </main>
    </div>
  );
}
