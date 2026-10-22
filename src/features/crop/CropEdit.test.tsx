import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  A4,
  areaRows,
  boxPt,
  candidate,
  clientAt,
  dragFromTo,
  drawFromTo,
  pickDrawTool,
  emitJobFinished,
  emitJobItem,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  moveTo,
  openPdfInCropTab,
  openSettings,
  overlay,
  pageImage,
  pressAt,
  releaseAt,
  resizeRoom,
  rowText,
} from "./cropTestSupport";

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const CHART: [number, number, number, number] = [50, 60, 250, 160];
const PHOTO: [number, number, number, number] = [100, 200, 300, 400];
const OUTPUT_DIR = { dirLabel: "figures" };

/** A page with one chart suggestion. */
async function openWithChart(handlers = {}) {
  return openPdfInCropTab({
    detect_layout: () => [candidate("chart", CHART)],
    ...handlers,
  });
}

/** Opens a page with the chart added, which makes it the selected area. */
async function openWithChartArea() {
  const calls = await openWithChart();
  fireEvent.click(screen.getByTestId("crop-candidate"));
  await flush();
  return calls;
}

function theArea(): HTMLElement {
  return screen.getByTestId("crop-area");
}

function handle(id: string): HTMLElement {
  const found = screen
    .queryAllByTestId("crop-handle")
    .find((element) => element.dataset.handle === id);
  if (found === undefined) {
    throw new Error(`no handle ${id}`);
  }
  return found;
}

function expectBox(
  element: HTMLElement,
  expected: readonly [number, number, number, number],
) {
  boxPt(element).forEach((value, index) => {
    expect(value).toBeCloseTo(expected[index], 3);
  });
}

/** The area with `key`; the page stacks them by size, not by key. */
function areaByKey(key: number): HTMLElement {
  const area = screen
    .getAllByTestId("crop-area")
    .find((element) => element.dataset.key === String(key));
  if (area === undefined) {
    throw new Error(`no area ${key}`);
  }
  return area;
}

describe("drawing an area", () => {
  it("adds a hand-picked area from a drag on an empty place, selects it and lists it", async () => {
    await openWithChart();
    expect(areaRows()).toHaveLength(0);

    drawFromTo(overlay(), [300, 300], [450, 380]);
    await flush();

    expect(areaRows()).toHaveLength(1);
    expect(rowText(areaRows()[0])).toContain("手で選んだ範囲");
    expectBox(theArea(), [300, 300, 450, 380]);
    expect(theArea()).toHaveAttribute("data-selected", "true");
    expect(theArea()).toHaveAttribute("data-kind", "manual");
  });

  it("draws toward the top left as well", async () => {
    await openWithChart();
    drawFromTo(overlay(), [450, 380], [300, 300]);
    expectBox(theArea(), [300, 300, 450, 380]);
  });

  it("shows the shape while the pointer moves, and adds nothing until it is released", async () => {
    await openWithChart();
    pickDrawTool();
    pressAt(overlay(), [300, 300]);
    moveTo([400, 350]);
    expectBox(screen.getByTestId("crop-draft"), [300, 300, 400, 350]);
    expect(areaRows()).toHaveLength(0);
    expect(screen.queryByTestId("crop-area")).toBeNull();

    moveTo([450, 380]);
    expectBox(screen.getByTestId("crop-draft"), [300, 300, 450, 380]);
    releaseAt([450, 380]);
    expect(screen.queryByTestId("crop-draft")).toBeNull();
    expect(areaRows()).toHaveLength(1);
  });

  it("stops at the edges of the page", async () => {
    await openWithChart();
    drawFromTo(overlay(), [300, 300], [9000, 9000]);
    expectBox(theArea(), [300, 300, A4.widthPt, A4.heightPt]);
  });

  it("adds nothing when the rectangle is smaller than 4 pt on a side", async () => {
    await openWithChart();
    drawFromTo(overlay(), [300, 300], [303, 400]);
    dragFromTo(overlay(), [300, 300], [400, 303]);
    expect(areaRows()).toHaveLength(0);
  });

  it("adds it even where it covers an area almost exactly", async () => {
    await openWithChartArea();
    // Starts outside the area and ends so that the rectangles are 0.9 alike.
    drawFromTo(overlay(), [49, 59], [251, 161]);
    await flush();
    expect(areaRows()).toHaveLength(2);
    expect(areaByKey(2)).toHaveAttribute("data-selected", "true");
  });

  it("draws from a press on a frame or a candidate, and adds not the frame or candidate", async () => {
    await openPdfInCropTab({
      list_page_images: () => [pageImage(PHOTO, 2)],
      detect_layout: () => [candidate("chart", CHART)],
    });
    drawFromTo(screen.getByTestId("crop-candidate"), [60, 70], [200, 300]);
    drawFromTo(screen.getByTestId("crop-photo-frame"), [150, 250], [500, 600]);
    await flush();

    expect(areaByKey(1)).toHaveAttribute("data-kind", "manual");
    expect(areaByKey(2)).toHaveAttribute("data-kind", "manual");
    expectBox(areaByKey(1), [60, 70, 200, 300]);
    expectBox(areaByKey(2), [150, 250, 500, 600]);
  });

  it("does not add a frame or a candidate when the click after a drag arrives", async () => {
    await openWithChart();
    const chart = screen.getByTestId("crop-candidate");
    drawFromTo(chart, [60, 70], [200, 300]);
    fireEvent.click(chart);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(theArea()).toHaveAttribute("data-kind", "manual");
  });

  it("does not deselect with the click after a drag on an empty place", async () => {
    await openWithChart();
    drawFromTo(overlay(), [300, 300], [450, 380]);
    fireEvent.click(overlay());
    expect(theArea()).toHaveAttribute("data-selected", "true");
  });

  it("still adds a frame or a candidate that is clicked after an earlier drag", async () => {
    await openWithChart();
    drawFromTo(overlay(), [300, 300], [450, 380]);
    // The browser's click for the first drag never came.
    const chart = screen.getByTestId("crop-candidate");
    pressAt(chart, [100, 100]);
    releaseAt([100, 100]);
    fireEvent.click(chart);
    await flush();
    expect(areaRows()).toHaveLength(2);
  });
});

describe("a press that does not move", () => {
  it("adds the frame or candidate as a click does", async () => {
    await openWithChart();
    const chart = screen.getByTestId("crop-candidate");
    pressAt(chart, [100, 100]);
    moveTo([101, 101]);
    releaseAt([101, 101]);
    fireEvent.click(chart);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(theArea()).toHaveAttribute("data-kind", "chart");
  });

  it("deselects the area on an empty place and draws nothing", async () => {
    await openWithChartArea();
    expect(theArea()).toHaveAttribute("data-selected", "true");
    pressAt(overlay(), [400, 400]);
    moveTo([401, 401]);
    releaseAt([401, 401]);
    fireEvent.click(overlay());
    expect(areaRows()).toHaveLength(1);
    expect(theArea()).toHaveAttribute("data-selected", "false");
  });

  it("is a drag from 4 CSS pixels of movement", async () => {
    await openWithChart();
    const scale = clientAt([1, 0]).clientX;
    pickDrawTool();
    pressAt(overlay(), [300, 300]);
    fireEvent.pointerMove(overlay(), {
      ...clientAt([300, 300]),
      clientX: clientAt([300, 300]).clientX + 3.9,
    });
    expect(screen.queryByTestId("crop-draft")).toBeNull();
    fireEvent.pointerMove(overlay(), {
      ...clientAt([300, 300]),
      clientX: clientAt([300, 300]).clientX + 4,
      clientY: clientAt([300, 300]).clientY + 40 * scale,
    });
    expect(screen.getByTestId("crop-draft")).toBeInTheDocument();
  });

  it("takes only the main button", async () => {
    await openWithChart();
    fireEvent.pointerDown(overlay(), { ...clientAt([300, 300]), button: 2 });
    moveTo([450, 380]);
    releaseAt([450, 380]);
    expect(areaRows()).toHaveLength(0);
  });
});

describe("moving an area", () => {
  it("moves it with a drag from inside, selecting it at the press, and changes the state on release only", async () => {
    await openWithChartArea();
    const before = areaRows()[0].querySelector("img")?.style.left;
    // Select nothing, then press the area.
    fireEvent.click(overlay());
    expect(theArea()).toHaveAttribute("data-selected", "false");

    pressAt(theArea(), [100, 100]);
    expect(theArea()).toHaveAttribute("data-selected", "true");
    moveTo([150, 130]);
    expectBox(theArea(), [100, 90, 300, 190]);
    expect(areaRows()[0].querySelector("img")?.style.left).toBe(before);

    releaseAt([150, 130]);
    await flush();
    expectBox(theArea(), [100, 90, 300, 190]);
    expect(areaRows()[0].querySelector("img")?.style.left).not.toBe(before);
  });

  it("keeps the size and stops at the edges of the page", async () => {
    await openWithChartArea();
    dragFromTo(theArea(), [100, 100], [9000, 9000]);
    expectBox(theArea(), [
      A4.widthPt - 200,
      A4.heightPt - 100,
      A4.widthPt,
      A4.heightPt,
    ]);
    dragFromTo(theArea(), [A4.widthPt - 100, A4.heightPt - 50], [-9000, -9000]);
    expectBox(theArea(), [0, 0, 200, 100]);
  });

  it("does not select another area or draw one when a press lands on an area over a frame", async () => {
    await openPdfInCropTab({
      list_page_images: () => [pageImage(PHOTO, 2)],
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    dragFromTo(theArea(), [200, 300], [210, 310]);
    await flush();
    expect(areaRows()).toHaveLength(1);
  });

  it("moves the smaller one where areas overlap, as it lies on top", async () => {
    await openWithChartArea();
    drawFromTo(overlay(), [100, 100], [400, 300]);
    const [lower, upper] = screen.getAllByTestId("crop-area");
    expect(upper).toBe(areaByKey(1));
    // The press lands on the topmost element, which is what the browser does.
    dragFromTo(upper, [150, 150], [160, 150]);
    expectBox(lower, [100, 100, 400, 300]);
    expectBox(upper, [60, 60, 260, 160]);
  });

  it("turns a photo into a hand-picked area without its overlay mark or pixel size", async () => {
    await openPdfInCropTab({
      list_page_images: () => [pageImage(PHOTO, 2)],
      check_on_top: () => true,
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(rowText(areaRows()[0])).toContain("写真");
    expect(rowText(areaRows()[0])).toContain("重なりあり");
    expect(rowText(areaRows()[0])).toContain("元の画素 100 × 80");

    dragFromTo(theArea(), [200, 300], [220, 300]);
    await flush();

    expect(rowText(areaRows()[0])).toContain("手で選んだ範囲");
    expect(rowText(areaRows()[0])).not.toContain("重なりあり");
    expect(rowText(areaRows()[0])).not.toContain("元の画素");
    expect(rowText(areaRows()[0])).toContain("150 dpi · 417 × 417");
  });

  it("does not drop an area that is dragged back to where it was", async () => {
    await openWithChartArea();
    pressAt(theArea(), [100, 100]);
    moveTo([200, 200]);
    moveTo([100, 100]);
    releaseAt([100, 100]);
    expectBox(theArea(), CHART);
  });
});

describe("resizing an area", () => {
  it("shows the eight handles of the selected area only", async () => {
    await openWithChartArea();
    expect(screen.getAllByTestId("crop-handle")).toHaveLength(8);

    fireEvent.click(overlay());
    expect(screen.queryAllByTestId("crop-handle")).toHaveLength(0);
  });

  it("puts them on the corners and edge middles", async () => {
    await openWithChartArea();
    const centre = (id: string): [number, number] => {
      const [x0, y0, x1, y1] = boxPt(handle(id));
      return [(x0 + x1) / 2, (y0 + y1) / 2];
    };
    expect(centre("nw")).toEqual([50, 60].map((v) => expect.closeTo(v, 3)));
    expect(centre("se")).toEqual([250, 160].map((v) => expect.closeTo(v, 3)));
    expect(centre("n")).toEqual([150, 60].map((v) => expect.closeTo(v, 3)));
    expect(centre("w")).toEqual([50, 110].map((v) => expect.closeTo(v, 3)));
  });

  it("resizes by a handle and changes the state on release only", async () => {
    await openWithChartArea();
    expect(rowText(areaRows()[0])).toContain("150 dpi · 417 × 208");

    pressAt(handle("se"), [250, 160]);
    moveTo([300, 200]);
    expectBox(theArea(), [50, 60, 300, 200]);
    expect(rowText(areaRows()[0])).toContain("417 × 208");
    releaseAt([300, 200]);
    await flush();

    expectBox(theArea(), [50, 60, 300, 200]);
    expect(rowText(areaRows()[0])).toContain("521 × 292");
    expect(screen.queryAllByTestId("crop-draft")).toHaveLength(0);
  });

  it("moves the handles with the area while it is dragged", async () => {
    await openWithChartArea();
    pressAt(handle("nw"), [50, 60]);
    moveTo([70, 80]);
    const [x0, y0] = boxPt(handle("nw"));
    expect(x0 + 8 / 2 / 0.95).toBeGreaterThan(65);
    expect(y0).toBeGreaterThan(70);
    releaseAt([70, 80]);
  });

  it("stops at the page, and at 4 pt from the opposite edge", async () => {
    await openWithChartArea();
    dragFromTo(handle("nw"), [50, 60], [-9000, -9000]);
    expectBox(theArea(), [0, 0, 250, 160]);
    dragFromTo(handle("se"), [250, 160], [-9000, -9000]);
    expectBox(theArea(), [0, 0, 4, 4]);
  });

  it("can start from a handle that lies under another area", async () => {
    await openWithChartArea();
    // Another area drawn over the first one's corner.
    drawFromTo(overlay(), [200, 120], [400, 300]);
    // Pressing the first area selects it, as the click after a press would.
    pressAt(areaByKey(1), [60, 70]);
    releaseAt([60, 70]);
    expect(screen.getAllByTestId("crop-handle")).toHaveLength(8);
    dragFromTo(handle("se"), [250, 160], [260, 170]);
    expectBox(areaByKey(1), [50, 60, 260, 170]);
    expect(screen.getAllByTestId("crop-area")).toHaveLength(2);
  });
});

describe("the pointer", () => {
  it("is the select tool's on the page, frames and suggestions, and a cross with the draw tool", async () => {
    await openPdfInCropTab({
      list_page_images: () => [pageImage(PHOTO, 2)],
      detect_layout: () => [candidate("chart", CHART)],
    });
    expect(overlay()).toHaveClass("is-editable");
    expect(overlay()).not.toHaveClass("is-drawing");
    expect(screen.getByTestId("crop-candidate")).toHaveClass("is-pressable");
    expect(screen.getByTestId("crop-photo-frame")).toHaveClass("is-pressable");
    pickDrawTool();
    expect(overlay()).toHaveClass("is-drawing");
  });

  it("is the drag's own while a drag lasts", async () => {
    await openWithChartArea();
    pressAt(handle("e"), [250, 110]);
    moveTo([260, 110]);
    expect(overlay()).toHaveClass("is-dragging");
    expect(overlay().style.cursor).toBe("ew-resize");
    releaseAt([260, 110]);
    expect(overlay()).not.toHaveClass("is-dragging");

    pressAt(theArea(), [100, 100]);
    moveTo([110, 100]);
    expect(overlay().style.cursor).toBe("move");
    releaseAt([110, 100]);

    pressAt(overlay(), [400, 400]);
    moveTo([450, 450]);
    expect(overlay()).toHaveClass("is-panning");
    releaseAt([450, 450]);
    expect(overlay()).not.toHaveClass("is-panning");

    pickDrawTool();
    pressAt(overlay(), [400, 400]);
    moveTo([450, 450]);
    expect(overlay().style.cursor).toBe("crosshair");
    releaseAt([450, 450]);
  });

  it.each([
    ["nw", "nwse-resize"],
    ["se", "nwse-resize"],
    ["ne", "nesw-resize"],
    ["sw", "nesw-resize"],
    ["n", "ns-resize"],
    ["s", "ns-resize"],
    ["e", "ew-resize"],
    ["w", "ew-resize"],
  ])("is %s-resize on the %s handle", async (id, cursor) => {
    await openWithChartArea();
    expect(handle(id).style.cursor).toBe(cursor);
  });

  it("captures the pointer at the press, on the element that was pressed", async () => {
    const capture = vi.spyOn(Element.prototype, "setPointerCapture");
    await openWithChart();
    const chart = screen.getByTestId("crop-candidate");
    pressAt(chart, [100, 150]);
    expect(capture).toHaveBeenCalledTimes(1);
    expect(capture).toHaveBeenCalledWith(1);
    expect(capture.mock.contexts[0]).toBe(chart);
    moveTo([300, 350]);
    releaseAt([300, 350]);
    capture.mockRestore();
  });
});

describe("calling a drag off", () => {
  async function dragged() {
    await openWithChartArea();
    pressAt(theArea(), [100, 100]);
    moveTo([200, 150]);
    expectBox(theArea(), [150, 110, 350, 210]);
  }

  it("is done by Esc, which keeps the area selected and changes nothing", async () => {
    await dragged();
    fireEvent.keyDown(theArea(), { key: "Escape" });
    expectBox(theArea(), CHART);
    expect(theArea()).toHaveAttribute("data-selected", "true");
    releaseAt([200, 150]);
    fireEvent.click(overlay());
    await flush();
    expectBox(theArea(), CHART);
    expect(theArea()).toHaveAttribute("data-selected", "true");
  });

  it("is done by pointercancel", async () => {
    await dragged();
    fireEvent.pointerCancel(overlay(), clientAt([200, 150]));
    expectBox(theArea(), CHART);
    releaseAt([200, 150]);
    expectBox(theArea(), CHART);
  });

  it("is done by turning to another page", async () => {
    await dragged();
    fireEvent.click(screen.getByRole("button", { name: /^ページ 2/ }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: /^ページ 1/ }));
    await flush();
    releaseAt([200, 150]);
    expectBox(theArea(), CHART);
  });

  it("is done by a change of the size the page is shown at", async () => {
    await dragged();
    resizeRoom(500, 700);
    releaseAt([200, 150]);
    await flush();
    expect(rowText(areaRows()[0])).toContain("417 × 208");
    const [x0] = boxPt(theArea());
    expect(x0).toBeCloseTo(50, 3);
  });

  it("is done by a conversion starting", async () => {
    await openPdfInCropTab({
      pick_output_dir: () => OUTPUT_DIR,
      detect_layout: () => [candidate("chart", CHART)],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    pressAt(theArea(), [100, 100]);
    moveTo([200, 150]);
    // The footer's button starts the export; the folder is picked first.
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    releaseAt([200, 150]);
    expectBox(theArea(), CHART);
  });

  it("leaves Esc to deselect when no drag is on", async () => {
    await openWithChartArea();
    fireEvent.keyDown(theArea(), { key: "Escape" });
    expect(theArea()).toHaveAttribute("data-selected", "false");
  });

  it("does not take Esc a press that did not move yet", async () => {
    await openWithChartArea();
    pressAt(theArea(), [100, 100]);
    fireEvent.keyDown(theArea(), { key: "Escape" });
    expect(theArea()).toHaveAttribute("data-selected", "false");
  });
});

describe("while a conversion runs", () => {
  it("shows no handles and draws and moves nothing", async () => {
    await openPdfInCropTab({
      pick_output_dir: () => OUTPUT_DIR,
      detect_layout: () => [candidate("chart", CHART)],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();

    expect(screen.queryAllByTestId("crop-handle")).toHaveLength(0);
    expect(overlay()).not.toHaveClass("is-editable");
    dragFromTo(overlay(), [300, 300], [450, 380]);
    dragFromTo(theArea(), [100, 100], [200, 150]);
    expect(areaRows()).toHaveLength(1);
    expectBox(theArea(), CHART);
    expect(screen.queryByTestId("crop-draft")).toBeNull();
  });

  it("still selects an area that is clicked", async () => {
    await openPdfInCropTab({
      pick_output_dir: () => OUTPUT_DIR,
      detect_layout: () => [candidate("chart", CHART)],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    fireEvent.click(overlay());
    expect(theArea()).toHaveAttribute("data-selected", "false");
    pressAt(theArea(), [100, 100]);
    releaseAt([100, 100]);
    fireEvent.click(theArea());
    expect(theArea()).toHaveAttribute("data-selected", "true");
  });
});

describe("the result of the last write", () => {
  async function exported() {
    await openPdfInCropTab({
      pick_output_dir: () => OUTPUT_DIR,
      detect_layout: () => [candidate("chart", CHART)],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    await emitJobItem({ id: 1, status: "ok", outputs: ["a.png"] });
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    expect(screen.getByRole("status")).toBeInTheDocument();
  }

  it("goes when an area is drawn", async () => {
    await exported();
    drawFromTo(overlay(), [300, 300], [450, 380]);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("goes when an area is moved, and the area counts as not exported", async () => {
    await exported();
    expect(rowText(areaRows()[0])).toContain("✓ 書き出し済み");
    dragFromTo(theArea(), [100, 100], [120, 100]);
    await flush();
    expect(screen.queryByRole("status")).toBeNull();
    expect(rowText(areaRows()[0])).not.toContain("書き出し済み");
  });

  it("stays when the drag ends where the area began", async () => {
    await exported();
    pressAt(theArea(), [100, 100]);
    moveTo([200, 100]);
    moveTo([100, 100]);
    releaseAt([100, 100]);
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(rowText(areaRows()[0])).toContain("✓ 書き出し済み");
  });

  it("stays while the drag is on, and when it is called off", async () => {
    await exported();
    pressAt(theArea(), [100, 100]);
    moveTo([200, 100]);
    expect(screen.getByRole("status")).toBeInTheDocument();
    fireEvent.keyDown(theArea(), { key: "Escape" });
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("makes the replace question count a moved area", async () => {
    await exported();
    openSettings();
    dragFromTo(theArea(), [100, 100], [120, 100]);
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "別の PDF を開く" }));
    await flush();
    expect(
      screen.getByText("書き出していない範囲が 1 個あります"),
    ).toBeInTheDocument();
  });
});
