"use client";

// Textarea.tsx
// Standard multiline text input area.

import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const textareaVariants = cva(
  "flex min-h-[80px] w-full rounded-md border px-3 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none",
  {
    variants: {
      variant: {
        default:
          "border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        dark: "border-white/10 bg-black text-white placeholder:text-white/35 transition-colors hover:border-white/20 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/40",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

type TextareaProps = React.ComponentProps<"textarea"> &
  VariantProps<typeof textareaVariants>;

const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, variant, ...props }, ref) => {
    return (
      <textarea
        data-slot="textarea"
        className={cn(textareaVariants({ variant }), className)}
        ref={ref}
        {...props}
      />
    );
  },
);
Textarea.displayName = "Textarea";

export { Textarea, textareaVariants };
