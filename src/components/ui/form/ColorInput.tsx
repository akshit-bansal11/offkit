// ColorInput.tsx
// Input component specialized for color hex values.

"use client";

import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const colorInputVariants = cva(
  "cursor-pointer rounded-xl border p-1 transition-colors hover:border-white/25 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 [&::-webkit-color-swatch-wrapper]:p-0 [&::-webkit-color-swatch]:rounded-lg [&::-webkit-color-swatch]:border-0 [&::-moz-color-swatch]:rounded-lg [&::-moz-color-swatch]:border-0",
  {
    variants: {
      variant: {
        default: "border-input bg-background",
        dark: "border-white/10 bg-black",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

type ColorInputProps = Omit<React.ComponentProps<"input">, "type"> &
  VariantProps<typeof colorInputVariants>;

const ColorInput = React.forwardRef<HTMLInputElement, ColorInputProps>(
  ({ className, variant, ...props }, ref) => {
    return (
      <input
        ref={ref}
        type="color"
        className={cn(colorInputVariants({ variant }), className)}
        {...props}
      />
    );
  },
);
ColorInput.displayName = "ColorInput";

export { ColorInput, colorInputVariants };
