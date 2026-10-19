// Slider.tsx
// Range selection slider built on the native range input, with a filled track.

"use client";

import * as React from "react";

import { cn } from "@/lib/utils";

const sliderClassName = cn(
  "h-6 w-full cursor-pointer appearance-none bg-transparent outline-none disabled:cursor-not-allowed disabled:opacity-50",
  "[&::-webkit-slider-thumb]:-mt-1.5 [&::-webkit-slider-thumb]:appearance-none",
  "[&::-webkit-slider-runnable-track]:h-2 [&::-webkit-slider-runnable-track]:rounded-full [&::-webkit-slider-runnable-track]:[background:linear-gradient(to_right,var(--primary)_var(--slider-fill),color-mix(in_srgb,var(--primary)_22%,transparent)_var(--slider-fill))]",
  "[&::-moz-range-track]:h-2 [&::-moz-range-track]:rounded-full [&::-moz-range-track]:[background:linear-gradient(to_right,var(--primary)_var(--slider-fill),color-mix(in_srgb,var(--primary)_22%,transparent)_var(--slider-fill))]",
  "[&::-webkit-slider-thumb]:h-5 [&::-webkit-slider-thumb]:w-5 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-background [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:shadow-md [&::-webkit-slider-thumb]:transition-transform [&::-webkit-slider-thumb]:duration-150 motion-reduce:[&::-webkit-slider-thumb]:transition-none",
  "[&::-moz-range-thumb]:h-5 [&::-moz-range-thumb]:w-5 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border-2 [&::-moz-range-thumb]:border-background [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:shadow-md [&::-moz-range-thumb]:transition-transform [&::-moz-range-thumb]:duration-150 motion-reduce:[&::-moz-range-thumb]:transition-none",
  "enabled:hover:[&::-webkit-slider-thumb]:scale-110 enabled:active:[&::-webkit-slider-thumb]:scale-95",
  "enabled:hover:[&::-moz-range-thumb]:scale-110 enabled:active:[&::-moz-range-thumb]:scale-95",
  "focus-visible:[&::-webkit-slider-thumb]:ring-4 focus-visible:[&::-webkit-slider-thumb]:ring-ring/40",
  "focus-visible:[&::-moz-range-thumb]:ring-4 focus-visible:[&::-moz-range-thumb]:ring-ring/40",
);

function Slider({
  className,
  min = 0,
  max = 100,
  step = 1,
  value,
  defaultValue,
  onChange,
  style,
  ...props
}: React.ComponentProps<"input"> & {
  value?: number;
}) {
  const [inner, setInner] = React.useState(Number(defaultValue ?? min));
  const current = value ?? inner;
  const span = Number(max) - Number(min);
  const fill = span > 0 ? ((current - Number(min)) / span) * 100 : 0;

  const fillStyle: React.CSSProperties & { "--slider-fill": string } = {
    ...style,
    "--slider-fill": `${Math.min(100, Math.max(0, fill))}%`,
  };

  return (
    <div
      className={cn("relative flex w-full select-none items-center", className)}
    >
      <input
        data-slot="slider"
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        defaultValue={value === undefined ? defaultValue : undefined}
        onChange={(event) => {
          setInner(event.target.valueAsNumber);
          onChange?.(event);
        }}
        className={sliderClassName}
        style={fillStyle}
        {...props}
      />
    </div>
  );
}

export { Slider };
