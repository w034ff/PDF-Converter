import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  A4,
  areaRows,
  boxPt,
  candidate,
  dragFromTo,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  overlay,
  pageImage,
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

const CHART: [number, number, number, number] = [50, 60, 250, 160];
const PHOTO: [number, number, number, number] = [300, 300, 500, 400];
const OUTPUT_DIR = { dirLabel: "figures" };

async function openWithAll(
  language: "ja" | "en" = "ja",
  handlers: Record<string, () => unknown> = {},
) {
  return openPdfInCropTab(
    {
      list_page_images: () => [pageImage(PHOTO, 2)],
      detect_layout: () => [candidate("chart", CHART)],
      pick_output_dir: () => OUTPUT_DIR,
      ...handlers,
    },
    undefined,
    language,
  );
}

function areaElements(): HTMLElement[] {
  return screen.queryAllByTestId("crop-area");
}

function tabbableKinds(): (string | undefined)[] {
  return Array.from(
    overlay().querySelectorAll<HTMLElement>("[tabindex='0']"),
  ).map((element) => element.dataset.testid);
}

/** A click on an empty place, which deselects. */
function tapEmptyPlace() {
  pressAt(overlay(), [550, 30]);
  releaseAt([550, 30]);
  fireEvent.click(overlay());
}

/** Draws `count` areas side by side, in the order of their keys, and deselects them. */
async function drawAreas(count: number) {
  for (let index = 0; index < count; index += 1) {
    const x = 20 + index * 120;
    dragFromTo(overlay(), [x, 500], [x + 100, 600]);
  }
  tapEmptyPlace();
  await flush();
}

function press(element: Element, key: string, init: KeyboardEventInit = {}) {
  return fireEvent.keyDown(element, { key, ...init });
}

function expectBox(
  element: HTMLElement,
  expected: readonly [number, number, number, number],
) {
  boxPt(element).forEach((value, index) => {
    expect(value).toBeCloseTo(expected[index], 3);
  });
}

describe("what the keyboard reaches", () => {
  it("is the suggestions, then the frames, then the areas, each a button", async () => {
    await openWithAll();
    await drawAreas(1);
    expect(tabbableKinds()).toEqual([
      "crop-candidate",
      "crop-photo-frame",
      "crop-area",
    ]);
    for (const element of overlay().querySelectorAll("[tabindex='0']")) {
      expect(element).toHaveAttribute("role", "button");
    }
  });

  it("leaves out the suggestions that are hidden", async () => {
    await openWithAll();
    await drawAreas(1);
    fireEvent.click(screen.getByRole("checkbox", { name: "候補" }));
    expect(tabbableKinds()).toEqual(["crop-photo-frame", "crop-area"]);
  });

  it("leaves out the frames and suggestions while a conversion runs, but not the areas", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    expect(tabbableKinds()).toEqual(["crop-area"]);
    expect(screen.getByTestId("crop-candidate")).toHaveAttribute(
      "tabindex",
      "-1",
    );
    expect(screen.getByTestId("crop-photo-frame")).toHaveAttribute(
      "tabindex",
      "-1",
    );
  });

  it("names the suggestion, the frame and the area", async () => {
    await openWithAll();
    expect(
      screen.getByRole("button", { name: "候補：グラフ" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "写真の枠" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "候補：グラフ" }));
    expect(
      screen.getByRole("button", { name: "範囲：グラフ" }),
    ).toBeInTheDocument();
  });

  it("names them in English", async () => {
    await openWithAll("en");
    expect(
      screen.getByRole("button", { name: "Suggestion: Chart" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Photo frame" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Suggestion: Chart" }));
    expect(
      screen.getByRole("button", { name: "Area: Chart" }),
    ).toBeInTheDocument();
  });

  it("marks the selected area as current", async () => {
    await openWithAll();
    await drawAreas(2);
    const [first, second] = areaElements();
    expect(first).not.toHaveAttribute("aria-current");
    fireEvent.click(second);
    expect(second).toHaveAttribute("aria-current", "true");
    expect(first).not.toHaveAttribute("aria-current");
  });
});

describe("Enter and Space", () => {
  it.each([
    ["Enter", "Enter"],
    ["Space", " "],
  ])(
    "%s on a suggestion adds its area and takes the focus to it",
    async (_name, key) => {
      await openWithAll();
      const chart = screen.getByTestId("crop-candidate");
      chart.focus();
      press(chart, key);
      await flush();

      expect(areaRows()).toHaveLength(1);
      expect(areaElements()[0]).toHaveAttribute("data-kind", "chart");
      expect(areaElements()[0]).toHaveAttribute("data-selected", "true");
      expect(areaElements()[0]).toHaveFocus();
    },
  );

  it("on a frame adds a photo area and takes the focus to it", async () => {
    await openWithAll();
    const frame = screen.getByTestId("crop-photo-frame");
    frame.focus();
    press(frame, "Enter");
    await flush();

    expect(areaElements()[0]).toHaveAttribute("data-kind", "photo");
    expect(areaElements()[0]).toHaveFocus();
  });

  it("takes the focus to the area that was there when the suggestion repeats it", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await drawAreas(1);
    const [chart, drawn] = areaElements();
    expect(chart).toHaveAttribute("data-selected", "false");
    drawn.focus();

    press(screen.getByTestId("crop-candidate"), "Enter");
    await flush();

    expect(areaRows()).toHaveLength(2);
    expect(chart).toHaveAttribute("data-selected", "true");
    expect(chart).toHaveFocus();
  });

  it("takes the focus to the area even when it was already selected", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    screen.getByTestId("crop-candidate").focus();
    press(screen.getByTestId("crop-candidate"), "Enter");
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(areaElements()[0]).toHaveFocus();
  });

  it("selects an area", async () => {
    await openWithAll();
    await drawAreas(2);
    const [first, second] = areaElements();
    first.focus();
    press(first, "Enter");
    expect(first).toHaveAttribute("data-selected", "true");
    press(second, " ");
    expect(second).toHaveAttribute("data-selected", "true");
    expect(first).toHaveAttribute("data-selected", "false");
  });

  it("does nothing on a frame or suggestion while a conversion runs, but selects an area", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    press(screen.getByTestId("crop-photo-frame"), "Enter");
    press(screen.getByTestId("crop-candidate"), " ");
    expect(areaRows()).toHaveLength(1);

    fireEvent.click(overlay());
    press(areaElements()[0], "Enter");
    expect(areaElements()[0]).toHaveAttribute("data-selected", "true");
  });
});

describe("the arrow keys", () => {
  it("move the area a point, or ten with Shift, and select it", async () => {
    await openWithAll();
    await drawAreas(1);
    const [area] = areaElements();
    expectBox(area, [20, 500, 120, 600]);
    expect(area).toHaveAttribute("data-selected", "false");

    expect(press(area, "ArrowRight")).toBe(false);
    expectBox(area, [21, 500, 121, 600]);
    expect(area).toHaveAttribute("data-selected", "true");

    press(area, "ArrowDown", { shiftKey: true });
    expectBox(area, [21, 510, 121, 610]);
    press(area, "ArrowLeft");
    press(area, "ArrowUp", { shiftKey: true });
    expectBox(area, [20, 500, 120, 600]);
  });

  it("change the state once for every press, repeats included", async () => {
    await openWithAll();
    await drawAreas(1);
    const [area] = areaElements();
    for (let count = 0; count < 5; count += 1) {
      press(area, "ArrowRight", { repeat: true });
    }
    expectBox(area, [25, 500, 125, 600]);
  });

  it("move the right edge with Alt and ← →, the bottom edge with Alt and ↑ ↓", async () => {
    await openWithAll();
    await drawAreas(1);
    const [area] = areaElements();
    expect(press(area, "ArrowRight", { altKey: true })).toBe(false);
    expectBox(area, [20, 500, 121, 600]);
    press(area, "ArrowLeft", { altKey: true, shiftKey: true });
    expectBox(area, [20, 500, 111, 600]);
    press(area, "ArrowDown", { altKey: true });
    expectBox(area, [20, 500, 111, 601]);
    press(area, "ArrowUp", { altKey: true, shiftKey: true });
    expectBox(area, [20, 500, 111, 591]);
  });

  it("stop at the edge of the page, change nothing and still stop the browser", async () => {
    await openWithAll();
    dragFromTo(overlay(), [-50, 100], [100, 200]);
    tapEmptyPlace();
    const [area] = areaElements();
    expectBox(area, [0, 100, 100, 200]);
    expect(area).toHaveAttribute("data-selected", "false");

    expect(press(area, "ArrowLeft")).toBe(false);
    expectBox(area, [0, 100, 100, 200]);
    expect(area).toHaveAttribute("data-selected", "false");

    dragFromTo(overlay(), [300, 700], [9000, 9000]);
    const edge = areaElements()[1];
    expectBox(edge, [300, 700, A4.widthPt, A4.heightPt]);
    press(edge, "ArrowRight", { shiftKey: true });
    press(edge, "ArrowDown");
    expectBox(edge, [300, 700, A4.widthPt, A4.heightPt]);
    press(edge, "ArrowRight", { altKey: true });
    expectBox(edge, [300, 700, A4.widthPt, A4.heightPt]);
  });

  it("stop 4 pt from the opposite edge", async () => {
    await openWithAll();
    dragFromTo(overlay(), [100, 100], [106, 150]);
    const [area] = areaElements();
    press(area, "ArrowLeft", { altKey: true, shiftKey: true });
    expectBox(area, [100, 100, 104, 150]);
    press(area, "ArrowLeft", { altKey: true });
    expectBox(area, [100, 100, 104, 150]);
  });

  it("turn a photo into a hand-picked area", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(areaElements()[0]).toHaveAttribute("data-kind", "photo");
    press(areaElements()[0], "ArrowRight");
    await flush();
    expect(areaElements()[0]).toHaveAttribute("data-kind", "manual");
  });

  it("do nothing while a conversion runs, though they stop the browser", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    const [area] = areaElements();
    pressAt(overlay(), [550, 30]);
    releaseAt([550, 30]);
    fireEvent.click(overlay());
    expect(area).toHaveAttribute("data-selected", "false");

    expect(press(area, "ArrowRight")).toBe(false);
    press(area, "ArrowDown", { altKey: true });
    expectBox(area, CHART);
    // Not even selected: nothing was asked of the state.
    expect(area).toHaveAttribute("data-selected", "false");
  });

  it("leave Ctrl and the other keys alone", async () => {
    await openWithAll();
    await drawAreas(1);
    const [area] = areaElements();
    expect(press(area, "ArrowRight", { ctrlKey: true })).toBe(true);
    expect(press(area, "a")).toBe(true);
    expectBox(area, [20, 500, 120, 600]);
  });
});

describe("Delete and Backspace", () => {
  it.each(["Delete", "Backspace"])(
    "%s removes the area and takes the focus to the next one, else the one before, else the view",
    async (key) => {
      await openWithAll();
      await drawAreas(3);
      const [first, second, third] = areaElements();
      second.focus();

      press(second, key);
      await flush();
      expect(areaRows()).toHaveLength(2);
      expect(third).toHaveFocus();

      press(third, key);
      await flush();
      expect(first).toHaveFocus();

      press(first, key);
      await flush();
      expect(areaRows()).toHaveLength(0);
      expect(screen.getByTestId("crop-viewport")).toHaveFocus();
    },
  );

  it("does nothing while a conversion runs", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    const [area] = areaElements();
    area.focus();
    expect(press(area, "Delete")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(area).toHaveFocus();
  });
});

describe("Esc", () => {
  it("deselects the area and leaves the focus where it was", async () => {
    await openWithAll();
    await drawAreas(1);
    const [area] = areaElements();
    area.focus();
    press(area, "Enter");
    expect(area).toHaveAttribute("data-selected", "true");

    press(area, "Escape");
    expect(area).toHaveAttribute("data-selected", "false");
    expect(area).toHaveFocus();
  });

  it("works wherever in the page's view the focus is", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    const viewport = screen.getByTestId("crop-viewport");
    viewport.focus();
    press(viewport, "Escape");
    expect(areaElements()[0]).toHaveAttribute("data-selected", "false");
    expect(viewport).toHaveFocus();
  });

  it("is left to the menu that is open", async () => {
    await openWithAll();
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    const item = screen.getByRole("menuitem", {
      name: "このページの候補をすべて追加",
    });
    press(item, "Escape");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(areaElements()[0]).toHaveAttribute("data-selected", "true");
  });
});
