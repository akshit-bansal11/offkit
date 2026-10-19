// NotSmallerNotice.tsx
// Status line shown when compressing would not make a PDF smaller, so nothing is offered for download.

import { NOT_SMALLER_MESSAGE } from "@/lib/pdf/compress";
import { cn } from "@/lib/utils";

export function NotSmallerNotice({ className }: { className?: string }) {
  return (
    <p
      role="status"
      className={cn("text-xs leading-relaxed text-warning", className)}
    >
      {NOT_SMALLER_MESSAGE}
    </p>
  );
}
