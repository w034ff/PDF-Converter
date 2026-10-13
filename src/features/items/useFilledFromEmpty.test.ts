import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useFilledFromEmpty } from "./useFilledFromEmpty";

describe("useFilledFromEmpty", () => {
  it("is false for a list that already has items on mount", () => {
    const { result, rerender } = renderHook(
      ({ isEmpty }) => useFilledFromEmpty(isEmpty),
      { initialProps: { isEmpty: false } },
    );
    expect(result.current).toBe(false);

    rerender({ isEmpty: false });
    expect(result.current).toBe(false);
  });

  it("becomes true when an empty list gets items and stays true", () => {
    const { result, rerender } = renderHook(
      ({ isEmpty }) => useFilledFromEmpty(isEmpty),
      { initialProps: { isEmpty: true } },
    );
    expect(result.current).toBe(false);

    rerender({ isEmpty: false });
    expect(result.current).toBe(true);

    rerender({ isEmpty: false });
    expect(result.current).toBe(true);
  });

  it("resets when the list is emptied, then counts a refill again", () => {
    const { result, rerender } = renderHook(
      ({ isEmpty }) => useFilledFromEmpty(isEmpty),
      { initialProps: { isEmpty: false } },
    );

    rerender({ isEmpty: true });
    expect(result.current).toBe(false);

    rerender({ isEmpty: false });
    expect(result.current).toBe(true);
  });
});
