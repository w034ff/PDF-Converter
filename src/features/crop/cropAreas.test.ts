import { describe, expect, it } from "vitest";
import { en, ja } from "../../i18n";
import type { PageImage } from "../../ipc";
import {
  CROP_PAGE_SIDE_STEPS,
  PAGE_VIEW_MAX_SIDE,
  type CropArea,
  type CropPageResult,
} from "../../state";
import {
  CROP_PREVIEW_HEIGHT,
  CROP_PREVIEW_WIDTH,
  areasByPage,
  areasInListOrder,
  exportSize,
  exportSizeText,
  previewMaxSide,
  previewPlacement,
} from "./cropAreas";

const A4 = { widthPt: 595, heightPt: 842 };

function area(overrides: Partial<CropArea> = {}): CropArea {
  return {
    key: 1,
    page: 1,
    kind: "figure",
    rectPt: [0, 0, 297.5, 210.5],
    imageIndex: null,
    checked: true,
    onTop: null,
    ...overrides,
  };
}

function image(overrides: Partial<PageImage> = {}): PageImage {
  return {
    imageIndex: 3,
    rectPt: [0, 0, 100, 80],
    pxWidth: 1241,
    pxHeight: 964,
    nativeSize: { width: 1241, height: 964, scaledDown: false },
    ...overrides,
  };
}

function done(...images: PageImage[]): CropPageResult<PageImage[]> {
  return { status: "done", value: images };
}

const SETTINGS = { dpi: 150, photoNative: true };

describe("previewMaxSide", () => {
  const preview = 44;

  it("takes the smallest step that gives the area's long side the pixels of the box", () => {
    // The area is a third of the page's long side: 44 px for it is 132 px for
    // the page, and with a device ratio of 2, 264 px.
    const rect: [number, number, number, number] = [0, 0, 100, 842 / 3];
    expect(previewMaxSide(rect, A4, preview, 1)).toBe(400);
    expect(previewMaxSide(rect, A4, preview, 2)).toBe(400);
    // 44 × 3 × 4 = 528 > 400.
    expect(previewMaxSide(rect, A4, preview, 4)).toBe(800);
  });

  it("asks for more of the page when the area is small", () => {
    // 1/20 of the long side: 44 × 20 = 880 → 1200.
    expect(previewMaxSide([0, 0, 42.1, 42.1], A4, preview, 1)).toBe(1200);
  });

  it("takes the longer side of the area, whichever way it lies", () => {
    expect(previewMaxSide([0, 0, 42.1, 200], A4, preview, 1)).toBe(
      previewMaxSide([0, 0, 200, 42.1], A4, preview, 1),
    );
  });

  it("is the largest side when no step is enough", () => {
    expect(previewMaxSide([0, 0, 2, 2], A4, preview, 2)).toBe(
      PAGE_VIEW_MAX_SIDE,
    );
  });

  it("is always one of the steps", () => {
    for (const long of [10, 55, 120, 300, 842]) {
      expect(CROP_PAGE_SIDE_STEPS).toContain(
        previewMaxSide([0, 0, long, long / 2], A4, preview, 1.5),
      );
    }
  });

  it("gives the area at least the pixels it needs", () => {
    const rect: [number, number, number, number] = [10, 10, 130, 90];
    const side = previewMaxSide(rect, A4, preview, 2);
    const areaLongPx = (120 / A4.heightPt) * side;
    expect(areaLongPx).toBeGreaterThanOrEqual(preview * 2);
  });
});

describe("previewPlacement", () => {
  it("fits a wide area to the box and centers it, and puts the page's corner where the area's is", () => {
    const { window, image } = previewPlacement([100, 200, 188, 234], A4);
    // 88 × 34 pt against a 44 × 34 box: scale 0.5.
    expect(window.width).toBeCloseTo(44);
    expect(window.height).toBeCloseTo(17);
    expect(window.left).toBeCloseTo(0);
    expect(window.top).toBeCloseTo((CROP_PREVIEW_HEIGHT - 17) / 2);
    expect(image.width).toBeCloseTo(A4.widthPt * 0.5);
    expect(image.height).toBeCloseTo(A4.heightPt * 0.5);
    expect(image.left).toBeCloseTo(-50);
    expect(image.top).toBeCloseTo(-100);
  });

  it("fits a tall area to the box", () => {
    const { window, image } = previewPlacement([0, 0, 17, 34], A4);
    expect(window.height).toBeCloseTo(CROP_PREVIEW_HEIGHT);
    expect(window.width).toBeCloseTo(17);
    expect(window.left).toBeCloseTo((CROP_PREVIEW_WIDTH - 17) / 2);
    expect(image.width).toBeCloseTo(A4.widthPt);
  });

  it("shows nothing of the page outside the area", () => {
    const { window, image } = previewPlacement([60, 340, 540, 480], A4);
    // The window is the area, so the area's corner is the window's corner.
    expect(image.left + (60 / A4.widthPt) * image.width).toBeCloseTo(0);
    expect(image.top + (340 / A4.heightPt) * image.height).toBeCloseTo(0);
    expect(image.left + (540 / A4.widthPt) * image.width).toBeCloseTo(
      window.width,
    );
    expect(image.top + (480 / A4.heightPt) * image.height).toBeCloseTo(
      window.height,
    );
  });
});

describe("exportSize", () => {
  it("is the rectangle's points at the resolution, rounded", () => {
    // 297.5 pt × 150 / 72 = 619.79…; 210.5 pt → 438.54…
    expect(exportSize(area(), undefined, SETTINGS)).toEqual({
      kind: "dpi",
      dpi: 150,
      width: 620,
      height: 439,
    });
  });

  it("is at least one pixel on each side", () => {
    expect(
      exportSize(area({ rectPt: [0, 0, 0.1, 0.1] }), undefined, SETTINGS),
    ).toMatchObject({ width: 1, height: 1 });
  });

  it("does not depend on the page's embedded images for a kind but photo", () => {
    expect(
      exportSize(area({ kind: "table" }), { status: "pending" }, SETTINGS),
    ).not.toBeNull();
  });

  describe("a photo", () => {
    const photo = area({
      kind: "photo",
      imageIndex: 3,
      rectPt: [0, 0, 100, 80],
    });

    it("is the image's own pixels", () => {
      expect(exportSize(photo, done(image()), SETTINGS)).toEqual({
        kind: "native",
        width: 1241,
        height: 964,
        scaledDown: false,
      });
    });

    it("is the size Rust gives, whatever shape the area has", () => {
      // The image is wider than tall and the area is taller than wide, as
      // when it is stretched on the page: only Rust knows it is not turned.
      const stretched = area({ ...photo, rectPt: [0, 0, 50, 200] });
      expect(exportSize(stretched, done(image()), SETTINGS)).toMatchObject({
        width: 1241,
        height: 964,
      });
      // And a turned image written taller than wide under a wider area.
      const turned = image({
        nativeSize: { width: 964, height: 1241, scaledDown: false },
      });
      expect(exportSize(photo, done(turned), SETTINGS)).toMatchObject({
        width: 964,
        height: 1241,
      });
    });

    it("says when Rust scaled it down", () => {
      const huge = image({
        pxWidth: 20_000,
        pxHeight: 10_000,
        nativeSize: { width: 14_142, height: 7_071, scaledDown: true },
      });
      expect(exportSize(photo, done(huge), SETTINGS)).toEqual({
        kind: "native",
        width: 14_142,
        height: 7_071,
        scaledDown: true,
      });
    });

    it("is the rectangle at the resolution when the image cannot be written as it is", () => {
      expect(
        exportSize(photo, done(image({ nativeSize: null })), SETTINGS),
      ).toEqual({ kind: "dpi", dpi: 150, width: 208, height: 167 });
    });

    it("is the rectangle at the resolution when original pixels are off, whatever is known of the page", () => {
      const off = { dpi: 300, photoNative: false };
      const expected = { kind: "dpi", dpi: 300, width: 417, height: 333 };
      expect(exportSize(photo, done(image()), off)).toEqual(expected);
      expect(exportSize(photo, undefined, off)).toEqual(expected);
    });

    it("is unknown while the page's images have not been asked for, are on their way or failed", () => {
      expect(exportSize(photo, undefined, SETTINGS)).toBeNull();
      expect(exportSize(photo, { status: "pending" }, SETTINGS)).toBeNull();
      expect(
        exportSize(
          photo,
          { status: "failed", error: { code: "WorkerCrashed", detail: null } },
          SETTINGS,
        ),
      ).toBeNull();
    });

    it("is unknown when the page has no such image", () => {
      expect(
        exportSize(photo, done(image({ imageIndex: 9 })), SETTINGS),
      ).toBeNull();
    });
  });
});

describe("exportSizeText", () => {
  it("writes the resolution and the size", () => {
    const size = { kind: "dpi", dpi: 150, width: 1650, height: 1200 } as const;
    expect(exportSizeText(ja, size)).toBe("150 dpi · 1650 × 1200");
    expect(exportSizeText(en, size)).toBe("150 dpi · 1650 × 1200");
  });

  it("writes the original pixels, and says when they are scaled down", () => {
    const native = {
      kind: "native",
      width: 1241,
      height: 964,
      scaledDown: false,
    } as const;
    expect(exportSizeText(ja, native)).toBe("元の画素 1241 × 964");
    expect(exportSizeText(ja, { ...native, scaledDown: true })).toBe(
      "元の画素を縮小 1241 × 964",
    );
    expect(exportSizeText(en, native)).toBe("Original pixels 1241 × 964");
  });
});

describe("areasByPage", () => {
  it("splits the areas in list order by page, leaving out pages without areas", () => {
    const areas = [
      area({ key: 1, page: 3 }),
      area({ key: 2, page: 1 }),
      area({ key: 3, page: 3 }),
      area({ key: 4, page: 1 }),
      area({ key: 5, page: 4 }),
    ];
    expect(
      areasByPage(areas).map(({ page, areas }) => [
        page,
        areas.map((a) => a.key),
      ]),
    ).toEqual([
      [1, [2, 4]],
      [3, [1, 3]],
      [4, [5]],
    ]);
  });

  it("has no pages without areas", () => {
    expect(areasByPage([])).toEqual([]);
  });
});

describe("areasInListOrder", () => {
  it("goes by page, and by key within a page", () => {
    const areas = [
      area({ key: 1, page: 3 }),
      area({ key: 2, page: 1 }),
      area({ key: 3, page: 3 }),
      area({ key: 4, page: 1 }),
    ];
    expect(areasInListOrder(areas).map((a) => a.key)).toEqual([2, 4, 1, 3]);
  });

  it("leaves the areas as they are", () => {
    const areas = [area({ key: 1, page: 3 }), area({ key: 2, page: 1 })];
    areasInListOrder(areas);
    expect(areas.map((a) => a.key)).toEqual([1, 2]);
  });
});
