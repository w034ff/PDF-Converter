import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createThumbnailCache } from "./thumbnailCache";

describe("createThumbnailCache", () => {
  const blob = new Blob(["png"]);
  let nextUrl = 0;
  const revokeObjectURL = vi.fn();

  beforeEach(() => {
    nextUrl = 0;
    revokeObjectURL.mockClear();
    vi.stubGlobal("URL", {
      ...URL,
      createObjectURL: () => `blob:${++nextUrl}`,
      revokeObjectURL,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("releases the least recently used past its capacity", () => {
    const cache = createThumbnailCache(2);
    cache.put("a", blob);
    cache.put("b", blob);
    cache.touch("a");
    cache.put("c", blob);

    expect(cache.peek("a")).toBe("blob:1");
    expect(cache.peek("b")).toBeNull();
    expect(cache.peek("c")).toBe("blob:3");
    expect(revokeObjectURL.mock.calls).toEqual([["blob:2"]]);
  });

  it("does not count peeking as a use", () => {
    const cache = createThumbnailCache(1);
    cache.put("a", blob);
    cache.peek("a");
    cache.put("b", blob);

    expect(cache.peek("a")).toBeNull();
  });

  it("keeps the URL it has when the same key comes twice", () => {
    const cache = createThumbnailCache(2);
    const first = cache.put("a", blob);

    expect(cache.put("a", blob)).toBe(first);
    expect(nextUrl).toBe(1);
  });

  it("releases everything on clear", () => {
    const cache = createThumbnailCache(2);
    cache.put("a", blob);
    cache.put("b", blob);
    cache.clear();

    expect(cache.peek("a")).toBeNull();
    expect(revokeObjectURL).toHaveBeenCalledTimes(2);
  });
});
