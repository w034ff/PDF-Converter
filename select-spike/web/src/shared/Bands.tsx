import type { Rect } from "../core/ocr";

export function Bands({ rects, scale, className }: { rects: Rect[]; scale: number; className: string }) {
  return (
    <div className={`bands ${className}`} data-count={rects.length}>
      {rects.map((r, i) => (
        <div
          key={i}
          style={{ left: r.x0 * scale, top: r.y0 * scale, width: (r.x1 - r.x0) * scale, height: (r.y1 - r.y0) * scale }}
        />
      ))}
    </div>
  );
}
