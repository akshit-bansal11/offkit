// Field.tsx
// Utility wrapper for form fields combining a label and input element.

import type { ComponentProps } from "react";
import * as React from "react";
import { cn } from "@/lib/utils";

export function FieldLabel({ className, ...props }: ComponentProps<"label">) {
  return (
    <label
      className={cn("text-sm font-medium text-foreground", className)}
      {...props}
    />
  );
}

export function TextInput({ className, ...props }: ComponentProps<"input">) {
  return (
    <input
      className={cn(
        "flex h-10 w-full surface-inset px-3 py-2 text-sm shadow-sm outline-none transition focus:border-white/20 focus:ring-2 focus:ring-white/10",
        className,
      )}
      {...props}
    />
  );
}

export function TextArea({ className, ...props }: ComponentProps<"textarea">) {
  return (
    <textarea
      className={cn(
        "min-h-[140px] w-full surface-inset px-4 py-3 text-sm shadow-sm outline-none transition focus:border-white/20 focus:ring-2 focus:ring-white/10",
        className,
      )}
      {...props}
    />
  );
}
