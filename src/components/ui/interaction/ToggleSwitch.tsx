// ToggleSwitch.tsx
// Interactive switch component for turning settings on and off.

"use client";

import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const toggleSwitchVariants = cva(
  "relative inline-flex shrink-0 cursor-pointer items-center rounded-full bg-[hsl(0,0%,7%)] outline-none transition-all active:scale-[0.97] motion-reduce:transition-none motion-reduce:transform-none disabled:pointer-events-none disabled:opacity-50 focus-visible:ring-[3px] focus-visible:ring-ring/50",
  {
    variants: {
      variant: {
        default:
          "shadow-[0px_2px_4px_0px_rgba(18,18,18,0.25),0px_4px_8px_0px_rgba(18,18,18,0.35)]",
        destructive:
          "shadow-[0px_2px_4px_0px_rgba(18,18,18,0.25),0px_4px_8px_0px_rgba(18,18,18,0.35)]",
        outline:
          "shadow-[0px_2px_4px_0px_rgba(18,18,18,0.25),0px_4px_8px_0px_rgba(18,18,18,0.35)]",
        secondary:
          "shadow-[0px_2px_4px_0px_rgba(18,18,18,0.25),0px_4px_8px_0px_rgba(18,18,18,0.35)]",
        ghost:
          "shadow-[0px_2px_4px_0px_rgba(18,18,18,0.25),0px_4px_8px_0px_rgba(18,18,18,0.35)]",
        link: "shadow-[0px_2px_4px_0px_rgba(18,18,18,0.25),0px_4px_8px_0px_rgba(18,18,18,0.35)]",
      },
      size: {
        sm: "h-7 w-14",
        default: "h-8 w-16",
        lg: "h-10 w-20",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

const toggleSwitchThumbVariants = cva(
  "absolute top-1/2 flex -translate-y-1/2 items-center justify-center rounded-full bg-[rgb(26,26,26)] shadow-[inset_4px_4px_4px_0px_rgba(64,64,64,0.25),inset_-4px_-4px_4px_0px_rgba(16,16,16,0.5)] transition-all duration-300 ease-out motion-reduce:transition-none",
  {
    variants: {
      size: {
        sm: "left-[0.18rem] size-[1.45rem] data-[state=checked]:left-[calc(100%-1.63rem)]",
        default:
          "left-[0.2rem] size-[1.65rem] data-[state=checked]:left-[calc(100%-1.85rem)]",
        lg: "left-[0.2rem] size-[2.1rem] data-[state=checked]:left-[calc(100%-2.3rem)]",
      },
      variant: {
        default: "",
        destructive: "",
        outline: "",
        secondary: "",
        ghost: "",
        link: "",
      },
    },
    defaultVariants: {
      size: "default",
      variant: "default",
    },
  },
);

const toggleSwitchIndicatorVariants = cva(
  "absolute top-1/2 -translate-y-1/2 rounded-full bg-[hsl(0,0%,50%)] shadow-[inset_0px_2px_1px_0px_hsl(0,0%,40%)] transition-opacity duration-300",
  {
    variants: {
      size: {
        sm: "right-[0.32rem] h-[0.16rem] w-[0.8rem]",
        default: "right-[0.4rem] h-[0.18rem] w-[0.9rem]",
        lg: "right-[0.45rem] h-[0.2rem] w-[1.1rem]",
      },
      variant: {
        default: "data-[state=checked]:opacity-0",
        destructive: "data-[state=checked]:opacity-0",
        outline: "data-[state=checked]:opacity-0",
        secondary: "data-[state=checked]:opacity-0",
        ghost: "data-[state=checked]:opacity-0",
        link: "data-[state=checked]:opacity-0",
      },
    },
    defaultVariants: {
      size: "default",
      variant: "default",
    },
  },
);

type ToggleSwitchProps = Omit<React.ComponentProps<"button">, "onChange"> &
  VariantProps<typeof toggleSwitchVariants> & {
    checked: boolean;
    onCheckedChange?: (checked: boolean) => void;
  };

function ToggleSwitch({
  className,
  variant,
  size,
  checked,
  onCheckedChange,
  onClick,
  type,
  ...props
}: ToggleSwitchProps) {
  const state = checked ? "checked" : "unchecked";

  return (
    <button
      type={type ?? "button"}
      role="switch"
      aria-checked={checked}
      data-state={state}
      className={cn(toggleSwitchVariants({ variant, size, className }))}
      onClick={(event) => {
        onClick?.(event);
        if (!event.defaultPrevented) {
          onCheckedChange?.(!checked);
        }
      }}
      {...props}
    >
      <span
        aria-hidden="true"
        className="pointer-events-none absolute inset-[0.1em] rounded-full border border-[hsl(0,0%,25%)]"
      />
      <span
        aria-hidden="true"
        data-state={state}
        className={cn(toggleSwitchIndicatorVariants({ size, variant }))}
      />
      <span
        aria-hidden="true"
        className={cn(
          "absolute top-1/2 flex -translate-y-1/2 items-center justify-center rounded-full bg-white shadow-[inset_0px_2px_2px_0px_hsl(0,0%,85%)] transition-opacity duration-300",
          size === "sm" && "left-[0.25rem] size-[1.1rem]",
          size === "default" && "left-[0.3rem] size-[1.3rem]",
          size === "lg" && "left-[0.35rem] size-[1.65rem]",
          checked ? "opacity-100" : "opacity-0",
        )}
      >
        <span
          className={cn(
            "rounded-full bg-[hsl(0,0%,7%)] shadow-[0px_2px_2px_0px_hsl(0,0%,85%)]",
            size === "sm" && "size-[0.8rem]",
            size === "default" && "size-[0.95rem]",
            size === "lg" && "size-[1.2rem]",
          )}
        />
      </span>
      <span
        aria-hidden="true"
        data-state={state}
        className={cn(toggleSwitchThumbVariants({ size, variant }))}
      >
        <span className="relative block h-full w-full rounded-full">
          <span className="absolute inset-[0.1em] rounded-full border border-[hsl(0,0%,50%)]" />
        </span>
      </span>
    </button>
  );
}

export { ToggleSwitch, toggleSwitchVariants };
