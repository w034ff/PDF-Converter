import { useEffect, useState } from "react";

/** How long typing must pause before a field error is shown. */
export const FIELD_ERROR_REVEAL_DELAY_MS = 600;

/**
 * Hook to delay revealing an error until typing has paused or the input has blurred.
 */
export function useDelayedReveal(
  value: string,
  hasError: boolean,
  delayMs = FIELD_ERROR_REVEAL_DELAY_MS,
): [boolean, () => void] {
  const [revealedFor, setRevealedFor] = useState<string | null>(null);

  useEffect(() => {
    if (!hasError) {
      return;
    }
    const timer = setTimeout(() => setRevealedFor(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, hasError, delayMs]);

  const isRevealed = hasError && revealedFor === value;
  const reveal = () => setRevealedFor(value);
  return [isRevealed, reveal];
}
