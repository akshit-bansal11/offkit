// Checkbox.tsx
// A custom checkbox control using the native checkbox input.

"use client";

import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const checkboxVariants = cva(
  "peer h-4 w-4 shrink-0 cursor-pointer rounded border accent-primary disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none",
  {
    variants: {
      variant: {
        default:
          "border-input bg-background ring-offset-background focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        dark: "border-white/15 bg-black ring-offset-0 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/40",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

type CheckboxProps = Omit<React.ComponentProps<"input">, "type"> &
  VariantProps<typeof checkboxVariants> & {
    onCheckedChange?: (checked: boolean) => void;
  };

const Checkbox = React.forwardRef<HTMLInputElement, CheckboxProps>(
  ({ className, variant, onChange, onCheckedChange, ...props }, ref) => {
    return (
      <input
        data-slot="checkbox"
        ref={ref}
        type="checkbox"
        className={cn(checkboxVariants({ variant }), className)}
        onChange={(event) => {
          onChange?.(event);
          onCheckedChange?.(event.target.checked);
        }}
        {...props}
      />
    );
  },
);
Checkbox.displayName = "Checkbox";

export { Checkbox, checkboxVariants };
