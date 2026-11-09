"use client";

// StatusRegion.tsx
// A visually hidden live region plus the hook that feeds it. Repeating the
// same message still announces: every call flips an invisible zero-width
// space on the end, so the region's text always changes.

import { useState } from "react";

export function useStatus() {
  const [message, setMessage] = useState("");
  const [flip, setFlip] = useState(false);

  function announce(text: string) {
    setMessage(text);
    setFlip((current) => !current);
  }

  return { message: flip ? `${message}​` : message, announce };
}

export function StatusRegion({ message }: { message: string }) {
  return (
    <p role="status" className="sr-only">
      {message}
    </p>
  );
}
