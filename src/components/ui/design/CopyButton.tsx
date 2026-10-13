// CopyButton.tsx
// Reusable button component that copies text to the clipboard and shows a success state.

"use client";

import { CheckCircle2, Copy } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/interaction/Button";
import { cn } from "@/lib/utils";

interface CopyButtonProps {
  value: string;
  label?: string;
  successLabel?: string;
  className?: string;
  variant?: React.ComponentProps<typeof Button>["variant"];
  size?: React.ComponentProps<typeof Button>["size"];
  disabled?: boolean;
}

export function CopyButton({
  value,
  label = "Copy",
  successLabel = "Copied",
  className,
  variant = "outline",
  size = "sm",
  disabled = false,
}: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    if (!value || disabled) {
      return;
    }

    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  return (
    <Button
      type="button"
      onClick={handleCopy}
      variant={variant}
      size={size}
      className={cn("gap-2", className)}
      disabled={disabled || !value}
    >
      {copied ? (
        <CheckCircle2 className="size-4" />
      ) : (
        <Copy className="size-4" />
      )}
      {copied ? successLabel : label}
    </Button>
  );
}
