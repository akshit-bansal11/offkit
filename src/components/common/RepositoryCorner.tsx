// RepositoryCorner.tsx
// Floating GitHub link button pinned to corner of the page.

import { GithubMark } from "@/components/common/GithubMark";
import { Button } from "@/components/ui/Button";
import { siteConfig } from "@/config/site";
import { cn } from "@/lib/utils";

interface RepositoryCornerProps {
  className?: string;
}

export function RepositoryCorner({ className }: RepositoryCornerProps) {
  return (
    <div className={cn("z-20", className)}>
      <Button
        asChild
        variant="outline"
        size="icon"
        className="size-11 rounded-full border-white/10 bg-white/[0.03] shadow-sm transition-transform hover:scale-110 hover:bg-white/[0.06] hover:border-white/15 active:scale-95 sm:size-12"
      >
        <a
          href={siteConfig.repositoryUrl}
          target="_blank"
          rel="noreferrer"
          aria-label="Offkit on GitHub"
        >
          <GithubMark className="size-5" />
        </a>
      </Button>
    </div>
  );
}
