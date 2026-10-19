// Select.tsx
// Interactive dropdown select menu using the native select element.

import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

type SelectOption =
  | string
  | {
      label: React.ReactNode;
      value: string;
      disabled?: boolean;
    };

const selectVariants = cva(
  "flex h-9 w-full appearance-none items-center justify-between whitespace-nowrap rounded-md border px-3 py-2 pr-8 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "border-input bg-background text-foreground shadow-sm ring-offset-background placeholder:text-muted-foreground hover:bg-accent/80 hover:text-accent-foreground cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-ring [&>option]:bg-neutral-800 [&>option]:text-foreground",
        dark: "border-white/10 bg-black text-white shadow-none placeholder:text-white/35 hover:border-white/20 focus:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/40 cursor-pointer [&>option]:bg-neutral-950 [&>option]:text-white",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

interface SelectProps
  extends React.ComponentProps<"select">,
    VariantProps<typeof selectVariants> {
  options?: readonly SelectOption[];
  placeholder?: string;
}

function Select({
  className,
  children,
  options,
  placeholder,
  variant,
  ...props
}: SelectProps) {
  return (
    <div className="relative">
      <select
        data-slot="select"
        className={cn(selectVariants({ variant }), className)}
        {...props}
      >
        {placeholder ? <option value="">{placeholder}</option> : null}
        {options?.map((option) => {
          const normalizedOption =
            typeof option === "string"
              ? { label: option, value: option }
              : option;

          return (
            <option
              key={normalizedOption.value}
              value={normalizedOption.value}
              disabled={normalizedOption.disabled}
            >
              {normalizedOption.label}
            </option>
          );
        })}
        {children}
      </select>
      <svg
        className={cn(
          "pointer-events-none absolute right-2 top-1/2 size-4 -translate-y-1/2",
          variant === "dark" ? "text-white/45" : "text-muted-foreground",
        )}
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </div>
  );
}

export { Select, selectVariants };
