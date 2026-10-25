import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IpcError } from "../../ipc";
import {
  areaRows,
  callsOf,
  candidate,
  dragFromTo,
  drawFromTo,
  emitJobFinished,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  moveTo,
  openPdfInCropTab,
  overlay,
  pageImage,
  pickDrawTool,
  pressAt,
  releaseAt,
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

/** A wrong "figure" round most of the page, with a photo and a chart inside it. */
const OUTER: [number, number, number, number] = [20, 20, 575, 800];
const PHOTO: [number, number, number, number] = [60, 60, 260, 200];
const CHART: [number, number, number, number] = [300, 300, 500, 400];

async function openNested() {
  return openPdfInCropTab({
    list_page_images: () => [pageImage(PHOTO, 3)],
    detect_layout: () => [
      candidate("figure", OUTER),
      candidate("chart", CHART),
    ],
    start_detect_all: () => undefined,
    pick_output_dir: () => ({ dirLabel: "figures" }),
  });
}

/** The test ids and kinds over the page, bottom first. */
function stack(): string[] {
  return Array.from(overlay().children)
    .map((element) =>
      element instanceof HTMLElement
        ? `${element.dataset.testid ?? ""}:${element.dataset.kind ?? ""}`
        : "",
    )
    .filter((entry) => entry !== "crop-handle:");
}

function viewport(): HTMLElement {
  return screen.getByTestId("crop-viewport");
}

const DRAWING_NOTE =
  "ドラッグで範囲を描きます（Enter でページ全体、Esc でやめる）";

/** Whether a drag would draw an area; the note under the page says so too. */
function isDrawing(): boolean {
  const drawing = overlay().classList.contains("is-drawing");
  expect(screen.queryByText(DRAWING_NOTE) !== null).toBe(drawing);
  return drawing;
}

function addMenuButton(): HTMLElement {
  return screen.getByRole("button", { name: "範囲を追加 ▾" });
}

function suggestion(kind: string): HTMLElement {
  const found = screen
    .getAllByTestId("crop-candidate")
    .find((element) => element.dataset.kind === kind);
  if (found === undefined) {
    throw new Error(`no ${kind} suggestion`);
  }
  return found;
}

describe("the stacking", () => {
  it("puts the smaller things over the bigger ones, so the inside can be pressed", async () => {
    await openNested();
    expect(stack()).toEqual([
      "crop-candidate:figure",
      "crop-photo-frame:",
      "crop-candidate:chart",
    ]);

    // An area round the whole outside does not cover what is inside it.
    fireEvent.click(suggestion("figure"));
    await flush();
    expect(stack()).toEqual([
      "crop-candidate:figure",
      "crop-area:figure",
      "crop-photo-frame:",
      "crop-candidate:chart",
    ]);
    fireEvent.click(suggestion("chart"));
    await flush();
    expect(areaRows()).toHaveLength(2);
  });

  it("is the order Tab goes through", async () => {
    await openNested();
    const tabbable = Array.from(
      overlay().querySelectorAll<HTMLElement>("[tabindex='0']"),
    ).map((element) => element.dataset.kind ?? element.dataset.testid);
    expect(tabbable).toEqual(["figure", "crop-photo-frame", "chart"]);
  });
});

describe("putting a wrong suggestion aside", () => {
  it("hides it with Delete, keeps it out of the menu's additions, and undoes", async () => {
    await openNested();
    const outer = suggestion("figure");
    outer.focus();
    expect(fireEvent.keyDown(outer, { key: "Delete" })).toBe(false);
    await flush();
    expect(
      screen
        .getAllByTestId("crop-candidate")
        .map((element) => element.dataset.kind),
    ).toEqual(["chart"]);
    expect(viewport()).toHaveFocus();
    expect(areaRows()).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "このページの候補をすべて追加" }),
    );
    await flush();
    expect(
      areaRows().map((row) => within(row).getByText("グラフ")),
    ).toHaveLength(1);

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();
    expect(areaRows()).toHaveLength(0);
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);
  });
});

describe("the page's menu", () => {
  function menuItems(): string[] {
    return within(screen.getByRole("menu", { name: "ページの操作" }))
      .getAllByRole("menuitem")
      .map((item) => item.textContent ?? "");
  }

  function choose(name: string) {
    fireEvent.click(screen.getByRole("menuitem", { name }));
  }

  it("opens on a suggestion instead of the browser's, to hide it", async () => {
    await openNested();
    expect(fireEvent.contextMenu(suggestion("figure"))).toBe(false);
    expect(menuItems()).toEqual([
      "この候補を隠す",
      "このページの隠した候補を戻す",
    ]);
    expect(
      [...screen.getByRole("menu").children].map((child) =>
        child.getAttribute("role"),
      ),
    ).toEqual(["menuitem", "separator", "menuitem"]);
    expect(
      screen.getByRole("menuitem", { name: "このページの隠した候補を戻す" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("menuitem", { name: "この候補を隠す" }),
    ).toHaveFocus();

    choose("この候補を隠す");
    await flush();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(
      screen
        .getAllByTestId("crop-candidate")
        .map((element) => element.dataset.kind),
    ).toEqual(["chart"]);
    expect(areaRows()).toHaveLength(0);
    expect(viewport()).toHaveFocus();
  });

  it("brings the page's hidden suggestions back from anywhere on the page, as a step", async () => {
    await openNested();
    fireEvent.contextMenu(suggestion("figure"));
    choose("この候補を隠す");
    fireEvent.contextMenu(suggestion("chart"));
    choose("この候補を隠す");
    await flush();
    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);

    fireEvent.contextMenu(screen.getByTestId("crop-photo-frame"));
    expect(menuItems()).toEqual(["このページの隠した候補を戻す"]);
    choose("このページの隠した候補を戻す");
    await flush();
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);
    expect(viewport()).toHaveFocus();

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();
    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);
    fireEvent.keyDown(document.body, { key: "y", ctrlKey: true });
    await flush();
    // From a suggestion's menu, the focus goes back to the suggestion.
    fireEvent.contextMenu(suggestion("figure"));
    choose("この候補を隠す");
    const chart = suggestion("chart");
    chart.focus();
    fireEvent.contextMenu(chart);
    choose("このページの隠した候補を戻す");
    await flush();
    expect(chart).toHaveFocus();
  });

  it("gives the focus back to the suggestion on Esc", async () => {
    await openNested();
    const chart = suggestion("chart");
    chart.focus();
    fireEvent.contextMenu(chart);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(chart).toHaveFocus();
  });

  it("takes the focus itself with nothing to choose, so Esc still closes it", async () => {
    await openNested();
    fireEvent.contextMenu(screen.getByTestId("crop-photo-frame"));
    const menu = screen.getByRole("menu");
    expect(menu).toHaveFocus();
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(viewport()).toHaveFocus();
  });

  it("keeps its keys from the page", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    fireEvent.contextMenu(suggestion("figure"));
    const item = screen.getByRole("menuitem", { name: "この候補を隠す" });
    fireEvent.keyDown(item, { key: "Delete" });
    fireEvent.keyDown(item, { key: "r" });
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(isDrawing()).toBe(false);
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("closes on a press outside it", async () => {
    await openNested();
    fireEvent.contextMenu(suggestion("figure"));
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);
  });

  it("does not open while drawing or while a conversion runs, nor does the browser's", async () => {
    await openNested();
    pickDrawTool();
    expect(fireEvent.contextMenu(suggestion("figure"))).toBe(false);
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.keyDown(document.body, { key: "Escape" });

    fireEvent.click(suggestion("chart"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    expect(fireEvent.contextMenu(suggestion("figure"))).toBe(false);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("drawing", () => {
  it("is off at first, and draws one area from the add menu before it stops", async () => {
    await openNested();
    expect(isDrawing()).toBe(false);
    pickDrawTool();
    expect(isDrawing()).toBe(true);

    dragFromTo(overlay(), [300, 500], [400, 600]);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(isDrawing()).toBe(false);
  });

  it("goes on after a rectangle too small to add", async () => {
    await openNested();
    pickDrawTool();
    dragFromTo(overlay(), [300, 500], [302, 600]);
    expect(areaRows()).toHaveLength(0);
    expect(isDrawing()).toBe(true);
  });

  it("draws over a suggestion, a frame and an area, and adds none of them by a press", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    pickDrawTool();
    fireEvent.click(suggestion("figure"));
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(areaRows()).toHaveLength(1);

    const [area] = screen.getAllByTestId("crop-area");
    dragFromTo(area, [320, 320], [450, 380]);
    await flush();
    expect(areaRows()).toHaveLength(2);
    expect(screen.queryAllByTestId("crop-handle")).toHaveLength(8);
  });

  it("shows no handles", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    expect(screen.getAllByTestId("crop-handle")).toHaveLength(8);
    pickDrawTool();
    expect(screen.queryAllByTestId("crop-handle")).toHaveLength(0);
  });

  it("starts with R, not V, and Esc stops it keeping the selection", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    fireEvent.keyDown(document.body, { key: "v" });
    expect(isDrawing()).toBe(false);
    fireEvent.keyDown(document.body, { key: "r" });
    expect(isDrawing()).toBe(true);
    fireEvent.keyDown(document.body, { key: "v" });
    expect(isDrawing()).toBe(true);

    fireEvent.keyDown(viewport(), { key: "Escape" });
    expect(isDrawing()).toBe(false);
    fireEvent.keyDown(document.body, { key: "R" });
    expect(isDrawing()).toBe(true);
    fireEvent.keyDown(viewport(), { key: "Escape" });
    expect(isDrawing()).toBe(false);
    expect(screen.getByTestId("crop-area")).toHaveAttribute(
      "data-selected",
      "true",
    );
  });

  it("makes the whole page an area with Enter, wherever the focus is, and stops", async () => {
    await openNested();
    pickDrawTool();
    const button = addMenuButton();
    button.focus();
    // Taken before the button, which would open the menu again.
    expect(fireEvent.keyDown(button, { key: "Enter" })).toBe(false);
    await flush();

    expect(screen.queryByRole("menu")).toBeNull();
    const [row] = areaRows();
    expect(rowText(row)).toContain("手で選んだ範囲");
    expect(rowText(row)).toContain("150 dpi · 1240 × 1754");
    expect(screen.getByTestId("crop-area")).toHaveAttribute(
      "data-selected",
      "true",
    );
    expect(isDrawing()).toBe(false);
  });

  it("leaves Enter alone when not drawing, and with a modifier", async () => {
    await openNested();
    expect(fireEvent.keyDown(document.body, { key: "Enter" })).toBe(true);
    pickDrawTool();
    fireEvent.keyDown(document.body, { key: "Enter", ctrlKey: true });
    await flush();
    expect(areaRows()).toHaveLength(0);
    expect(isDrawing()).toBe(true);
  });

  it("stops when a conversion starts, and cannot start while it runs", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    pickDrawTool();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    expect(isDrawing()).toBe(false);
    expect(addMenuButton()).toBeDisabled();
    fireEvent.keyDown(document.body, { key: "r" });
    expect(isDrawing()).toBe(false);

    // Nor does R pressed then start it for after the conversion.
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    expect(isDrawing()).toBe(false);
  });
});

describe("dragging the page", () => {
  it("scrolls the view from an empty place or a suggestion, and adds nothing", async () => {
    await openNested();
    viewport().scrollLeft = 100;
    viewport().scrollTop = 200;
    dragFromTo(overlay(), [300, 500], [290, 480]);
    expect(viewport().scrollLeft).toBeGreaterThan(100);
    expect(viewport().scrollTop).toBeGreaterThan(200);

    const left = viewport().scrollLeft;
    dragFromTo(suggestion("chart"), [400, 350], [420, 350]);
    await flush();
    expect(viewport().scrollLeft).toBeLessThan(left);
    expect(areaRows()).toHaveLength(0);
  });

  it("works while a conversion runs", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    expect(addMenuButton()).toBeDisabled();
    viewport().scrollTop = 200;
    pressAt(overlay(), [300, 600]);
    moveTo([300, 560]);
    releaseAt([300, 560]);
    expect(viewport().scrollTop).toBeGreaterThan(200);
  });
});

describe("the list of keys", () => {
  it("opens from its button and gives the focus back to it", async () => {
    await openNested();
    const button = screen.getByRole("button", { name: "キー操作" });
    expect(button).toHaveAttribute("title", "キー操作（?）");
    button.focus();
    fireEvent.click(button);
    const dialog = screen.getByRole("dialog");
    expect(
      within(dialog).getByText("元に戻す").closest("tr"),
    ).toHaveTextContent("Ctrl+Z");

    fireEvent.keyDown(dialog, { key: "Escape" });
    await flush();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(button).toHaveFocus();
  });

  it("opens with ?, and the other keys wait while it is open", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    fireEvent.keyDown(document.body, { key: "?" });
    const dialog = screen.getByRole("dialog");
    fireEvent.keyDown(dialog, { key: "z", ctrlKey: true });
    fireEvent.keyDown(dialog, { key: "r" });
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(isDrawing()).toBe(false);

    fireEvent.click(within(dialog).getByRole("button", { name: "閉じる" }));
    await flush();
    expect(viewport()).toHaveFocus();
  });
});

describe("the list of areas while it is empty", () => {
  it("offers to add the suggestions on all pages", async () => {
    const calls = await openNested();
    fireEvent.click(
      screen.getByRole("button", { name: "全ページの候補を追加" }),
    );
    await flush();
    expect(callsOf(calls, "start_detect_all")).toHaveLength(1);
  });

  it("cannot while the analysis is unavailable", async () => {
    const unavailable: IpcError = { code: "LayoutUnavailable", detail: null };
    await openPdfInCropTab({
      detect_layout: () => {
        throw unavailable;
      },
    });
    await flush();
    expect(
      screen.getByRole("button", { name: "全ページの候補を追加" }),
    ).toBeDisabled();
  });

  it("is gone once there is an area", async () => {
    await openNested();
    drawFromTo(overlay(), [300, 500], [400, 600]);
    await flush();
    expect(
      screen.queryByRole("button", { name: "全ページの候補を追加" }),
    ).toBeNull();
  });
});
