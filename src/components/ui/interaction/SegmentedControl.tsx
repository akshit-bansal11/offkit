// SegmentedControl.tsx
// UI component for selecting between multiple mutually exclusive options.

"use client";

import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

type SegmentedControlOption = {
  label: React.ReactNode;
  value: string;
  disabled?: boolean;
};

const segmentedControlVariants = cva(
  "inline-flex max-w-full flex-wrap items-center rounded-lg border p-1",
  {
    variants: {
      variant: {
        default: "border bg-background/50",
        dark: "border-white/10 bg-background/40",
      },
      size: {
        sm: "gap-1",
        default: "gap-1",
        lg: "gap-1.5",
      },
      fullWidth: {
        true: "w-full",
        false: "w-auto",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
      fullWidth: false,
    },
  },
);

const segmentedControlItemVariants = cva(
  "inline-flex items-center justify-center rounded-md font-medium transition-colors outline-none disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "text-muted-foreground hover:bg-accent/70 hover:text-accent-foreground data-[state=active]:bg-primary data-[state=active]:text-primary-foreground",
        dark: "text-muted-foreground hover:bg-background/80 hover:text-foreground data-[state=active]:bg-primary data-[state=active]:text-primary-foreground",
      },
      size: {
        sm: "h-7 px-2.5 text-xs",
        default: "h-8 px-3 text-sm",
        lg: "h-9 px-4 text-sm",
      },
      fullWidth: {
        true: "flex-1",
        false: "flex-none",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
      fullWidth: false,
    },
  },
);

type SegmentedControlProps = Omit<React.ComponentProps<"div">, "onChange"> &
  VariantProps<typeof segmentedControlVariants> & {
    value: string;
    options: readonly SegmentedControlOption[];
    onValueChange?: (value: string) => void;
    optionClassName?: string;
  };

function SegmentedControl({
  className,
  optionClassName,
  options,
  value,
  onValueChange,
  variant,
  size,
  fullWidth,
  ...props
}: SegmentedControlProps) {
  return (
    <div
      role="group"
      className={cn(
        segmentedControlVariants({ variant, size, fullWidth }),
        className,
      )}
      {...props}
    >
      {options.map((option) => {
        const active = option.value === value;

        return (
          <button
            key={option.value}
            type="button"
            data-state={active ? "active" : "inactive"}
            aria-pressed={active}
            className={cn(
              segmentedControlItemVariants({ variant, size, fullWidth }),
              optionClassName,
            )}
            disabled={option.disabled}
            onClick={() => {
              if (!option.disabled) {
                onValueChange?.(option.value);
              }
            }}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

export {
  SegmentedControl,
  type SegmentedControlOption,
  segmentedControlItemVariants,
  segmentedControlVariants,
};
