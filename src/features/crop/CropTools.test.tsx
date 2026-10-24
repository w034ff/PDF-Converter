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

function tool(name: "選ぶ" | "描く"): HTMLElement {
  return screen.getByRole("button", { name });
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

describe("the tools", () => {
  it("start on select, and draw one area with draw before going back", async () => {
    await openNested();
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(tool("描く"));
    expect(tool("描く")).toHaveAttribute("aria-pressed", "true");

    dragFromTo(overlay(), [300, 500], [400, 600]);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");
  });

  it("stay on draw after a rectangle too small to add", async () => {
    await openNested();
    pickDrawTool();
    dragFromTo(overlay(), [300, 500], [302, 600]);
    expect(areaRows()).toHaveLength(0);
    expect(tool("描く")).toHaveAttribute("aria-pressed", "true");
  });

  it("draw over a suggestion, a frame and an area, and add none of them by a press", async () => {
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

  it("show no handles while drawing", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    expect(screen.getAllByTestId("crop-handle")).toHaveLength(8);
    pickDrawTool();
    expect(screen.queryAllByTestId("crop-handle")).toHaveLength(0);
  });

  it("are picked with V and R, and Esc leaves draw keeping the selection", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    fireEvent.keyDown(document.body, { key: "r" });
    expect(tool("描く")).toHaveAttribute("aria-pressed", "true");
    fireEvent.keyDown(document.body, { key: "v" });
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");

    fireEvent.keyDown(document.body, { key: "R" });
    fireEvent.keyDown(viewport(), { key: "Escape" });
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("crop-area")).toHaveAttribute(
      "data-selected",
      "true",
    );
  });

  it("cannot be draw while a conversion runs", async () => {
    await openNested();
    fireEvent.click(suggestion("chart"));
    await flush();
    pickDrawTool();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");
    expect(tool("描く")).toBeDisabled();
    fireEvent.keyDown(document.body, { key: "r" });
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");

    // Nor does R pressed then pick it for after the conversion.
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");
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
    expect(tool("描く")).toBeDisabled();
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
    expect(tool("選ぶ")).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(within(dialog).getByRole("button", { name: "閉じる" }));
    await flush();
    expect(viewport()).toHaveFocus();
  });
});

describe("the list of areas while it is empty", () => {
  it("offers to add the suggestions on all pages", async () => {
    const calls = await openNested();
    expect(
      screen.getByText(/全ページの候補をまとめて加える/),
    ).toBeInTheDocument();
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
