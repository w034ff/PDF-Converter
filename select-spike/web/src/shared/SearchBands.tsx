import { useMemo } from "react";
import type { Flat } from "../core/ocr";
import { search } from "../core/search";
import { bands } from "../core/select";
import { Bands } from "./Bands";

export function SearchBands({ flat, query, scale }: { flat: Flat; query: string; scale: number }) {
  const rects = useMemo(() => search(flat, query).flatMap(([a, b]) => bands(flat, a, b)), [flat, query]);
  return <Bands rects={rects} scale={scale} className="hit" />;
}
