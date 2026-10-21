import { formatMessage, type Translations } from "../../i18n";
import type { PageImage, PageSizePt } from "../../ipc";
import {
  CROP_PAGE_SIDE_STEPS,
  MAX_RENDER_PIXELS,
  PAGE_VIEW_MAX_SIDE,
  type CropArea,
  type CropPageResult,
  type RectPt,
} from "../../state";

/** The box of a row's preview in the list of areas, in CSS pixels (mockup `CropA`). */
export const CROP_PREVIEW_WIDTH = 44;
export const CROP_PREVIEW_HEIGHT = 34;

/** The longer side of the preview box. */
const CROP_PREVIEW_LONG_SIDE = Math.max(
  CROP_PREVIEW_WIDTH,
  CROP_PREVIEW_HEIGHT,
);

/** The longer side of a rectangle of the page, in points. */
function longSide(width: number, height: number): number {
  return Math.max(width, height);
}

/**
 * The `maxSide` to ask `get_thumbnail` for the page that holds `rectPt`, so
 * that the area's longer side has at least `previewSidePx` ×
 * `devicePixelRatio` pixels: the smallest of `CROP_PAGE_SIDE_STEPS` that
 * does, or `PAGE_VIEW_MAX_SIDE` when none does (design §10.1).
 */
export function previewMaxSide(
  rectPt: RectPt,
  pageSize: PageSizePt,
  previewSidePx: number,
  devicePixelRatio: number,
): number {
  const areaLong = longSide(rectPt[2] - rectPt[0], rectPt[3] - rectPt[1]);
  if (areaLong <= 0) {
    return PAGE_VIEW_MAX_SIDE;
  }
  const pageLong = longSide(pageSize.widthPt, pageSize.heightPt);
  const wanted = Math.ceil(
    (previewSidePx * devicePixelRatio * pageLong) / areaLong,
  );
  return (
    CROP_PAGE_SIDE_STEPS.find((side) => side >= wanted) ?? PAGE_VIEW_MAX_SIDE
  );
}

/** `previewMaxSide` for the preview box of a row. */
export function areaPreviewMaxSide(
  area: CropArea,
  pageSize: PageSizePt,
  devicePixelRatio: number,
): number {
  return previewMaxSide(
    area.rectPt,
    pageSize,
    CROP_PREVIEW_LONG_SIDE,
    devicePixelRatio,
  );
}

/** A box inside another, in CSS pixels. */
export interface PreviewBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Where the area and the page lie in the preview box so that only the area
 * shows.
 */
export interface PreviewPlacement {
  /** The area, scaled to fit the preview box and centered in it. It clips what is outside the area. */
  window: PreviewBox;
  /** The page's picture inside `window`, shifted so that the area fills it. */
  image: PreviewBox;
}

/**
 * Places the page's picture so that the area `rectPt` is all that shows: the
 * area is scaled to fit the preview box (`CROP_PREVIEW_WIDTH` ×
 * `CROP_PREVIEW_HEIGHT`) and centered in it, and the page is scaled alike and
 * shifted to put the area's corner at the window's.
 */
export function previewPlacement(
  rectPt: RectPt,
  pageSize: PageSizePt,
): PreviewPlacement {
  const [x0, y0, x1, y1] = rectPt;
  const areaWidth = Math.max(x1 - x0, Number.EPSILON);
  const areaHeight = Math.max(y1 - y0, Number.EPSILON);
  const scale = Math.min(
    CROP_PREVIEW_WIDTH / areaWidth,
    CROP_PREVIEW_HEIGHT / areaHeight,
  );
  const width = areaWidth * scale;
  const height = areaHeight * scale;
  return {
    window: {
      left: (CROP_PREVIEW_WIDTH - width) / 2,
      top: (CROP_PREVIEW_HEIGHT - height) / 2,
      width,
      height,
    },
    image: {
      left: -x0 * scale,
      top: -y0 * scale,
      width: pageSize.widthPt * scale,
      height: pageSize.heightPt * scale,
    },
  };
}

/**
 * The size to write a `width` × `height` image in: its own, or when that
 * exceeds `maxPixels`, scaled by one factor to fit. The same arithmetic as
 * `photo_output_size` in `crates/worker/src/crop.rs`.
 */
export function photoOutputSize(
  width: number,
  height: number,
  maxPixels: number = MAX_RENDER_PIXELS,
): { width: number; height: number; scaledDown: boolean } {
  const limit = Math.max(maxPixels, 1);
  const total = width * height;
  if (total <= limit) {
    return { width, height, scaledDown: false };
  }
  let factor = Math.sqrt(limit / total);
  for (;;) {
    const outWidth = Math.max(Math.round(width * factor), 1);
    const outHeight = Math.max(Math.round(height * factor), 1);
    // Rounding both sides up can overshoot by a row or a column.
    if (outWidth * outHeight <= limit) {
      return { width: outWidth, height: outHeight, scaledDown: true };
    }
    factor *= 0.999;
  }
}

/** What a row says about the file an area is written to. */
export type ExportSize =
  | { kind: "dpi"; dpi: number; width: number; height: number }
  | { kind: "native"; width: number; height: number; scaledDown: boolean };

export interface ExportSizeSettings {
  dpi: number;
  /** Whether a photo is written in the pixels of its embedded image. */
  photoNative: boolean;
}

/**
 * The size of the image an area is written as, or `null` while it is not
 * known (design §10.1): a photo whose page's embedded images have not
 * arrived, were not found or lack its image.
 *
 * A rectangle is `round(pt × dpi ÷ 72)` on each side, as `RenderRegion` has
 * it. A photo that can be written in its own pixels (`native`, and
 * `photoNative` on) takes the image's pixels, turned if the area's
 * orientation is the other way round (a quarter turn), and scaled down when
 * they exceed `MAX_RENDER_PIXELS`. A photo that is cut or masked can come out
 * smaller than this, as only what shows is written.
 */
export function exportSize(
  area: CropArea,
  images: CropPageResult<PageImage[]> | undefined,
  settings: ExportSizeSettings,
): ExportSize | null {
  const dpiSize = (): ExportSize => {
    const scale = settings.dpi / 72;
    return {
      kind: "dpi",
      dpi: settings.dpi,
      width: Math.max(Math.round((area.rectPt[2] - area.rectPt[0]) * scale), 1),
      height: Math.max(
        Math.round((area.rectPt[3] - area.rectPt[1]) * scale),
        1,
      ),
    };
  };

  if (
    area.kind !== "photo" ||
    area.imageIndex === null ||
    !settings.photoNative
  ) {
    return dpiSize();
  }
  if (images?.status !== "done") {
    return null;
  }
  const image = images.value.find(
    (candidate) => candidate.imageIndex === area.imageIndex,
  );
  if (image === undefined) {
    return null;
  }
  if (!image.native) {
    return dpiSize();
  }
  const scaled = photoOutputSize(image.pxWidth, image.pxHeight);
  const areaWidth = area.rectPt[2] - area.rectPt[0];
  const areaHeight = area.rectPt[3] - area.rectPt[1];
  const turned =
    (areaWidth > areaHeight && image.pxWidth < image.pxHeight) ||
    (areaWidth < areaHeight && image.pxWidth > image.pxHeight);
  return {
    kind: "native",
    width: turned ? scaled.height : scaled.width,
    height: turned ? scaled.width : scaled.height,
    scaledDown: scaled.scaledDown,
  };
}

/** A size as a row writes it: "150 dpi · 1650 × 1200", "元の画素 1241 × 964". */
export function exportSizeText(t: Translations, size: ExportSize): string {
  const { size: templates } = t.crop.ranges;
  if (size.kind === "dpi") {
    return formatMessage(templates.dpi, {
      dpi: size.dpi,
      width: size.width,
      height: size.height,
    });
  }
  return formatMessage(
    size.scaledDown ? templates.nativeScaled : templates.native,
    { width: size.width, height: size.height },
  );
}

/** The areas in the order the list shows them: by page, and by `key` within a page. */
export function areasInListOrder(areas: readonly CropArea[]): CropArea[] {
  return [...areas].sort((a, b) => a.page - b.page || a.key - b.key);
}
