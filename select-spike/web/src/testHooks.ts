// Exposes the page model to the test scripts (tests/*.mjs). Not part of the feature.
import type { OcrPage } from "./core/ocr";
import { flatten, pageText, textOf } from "./core/ocr";
import { search } from "./core/search";
import { bands, caretAt } from "./core/select";

export function expose(page: OcrPage, scale: number) {
  const flat = flatten(page);
  Object.assign(window, {
    __spike: { page, flat, scale, pageText: pageText(page), textOf: (a: number, b: number) => textOf(flat, a, b), search: (q: string) => search(flat, q), bands: (a: number, b: number) => bands(flat, a, b), caretAt: (x: number, y: number) => caretAt(flat, x, y) },
  });
}
