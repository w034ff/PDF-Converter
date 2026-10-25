import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  A4,
  areaRows,
  boxPt,
  candidate,
  drawFromTo,
  toggleCandidates,
  viewMenuButton,
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

/** The area with `key`; the page stacks them by size, not by key. */
function areaByKey(key: number): HTMLElement {
  const area = areaElements().find(
    (element) => element.dataset.key === String(key),
  );
  if (area === undefined) {
    throw new Error(`no area ${key}`);
  }
  return area;
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
    drawFromTo(overlay(), [x, 500], [x + 100, 600]);
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
  it("follows the stacking, the biggest first, each a button", async () => {
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
    toggleCandidates();
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
    drawFromTo(overlay(), [-50, 100], [100, 200]);
    tapEmptyPlace();
    const [area] = areaElements();
    expectBox(area, [0, 100, 100, 200]);
    expect(area).toHaveAttribute("data-selected", "false");

    expect(press(area, "ArrowLeft")).toBe(false);
    expectBox(area, [0, 100, 100, 200]);
    expect(area).toHaveAttribute("data-selected", "false");

    drawFromTo(overlay(), [300, 700], [9000, 9000]);
    const edge = areaByKey(2);
    expectBox(edge, [300, 700, A4.widthPt, A4.heightPt]);
    press(edge, "ArrowRight", { shiftKey: true });
    press(edge, "ArrowDown");
    expectBox(edge, [300, 700, A4.widthPt, A4.heightPt]);
    press(edge, "ArrowRight", { altKey: true });
    expectBox(edge, [300, 700, A4.widthPt, A4.heightPt]);
  });

  it("stop 4 pt from the opposite edge", async () => {
    await openWithAll();
    drawFromTo(overlay(), [100, 100], [106, 150]);
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
  function viewport(): HTMLElement {
    return screen.getByTestId("crop-viewport");
  }

  it.each(["Delete", "Backspace"])(
    "%s removes the focused area and takes the focus to the view, not to another area",
    async (key) => {
      await openWithAll();
      await drawAreas(3);
      const [first, second, third] = areaElements();
      second.focus();

      press(second, key);
      await flush();
      expect(areaRows()).toHaveLength(2);
      expect(viewport()).toHaveFocus();
      expect(first).not.toHaveFocus();
      expect(third).not.toHaveFocus();
      expect(
        areaElements().filter((area) => area.dataset.selected === "true"),
      ).toHaveLength(0);
    },
  );

  it("removes the area just added by pressing a suggestion, which keeps the focus", async () => {
    await openWithAll();
    const suggestion = screen.getByTestId("crop-candidate");
    suggestion.focus();
    fireEvent.click(suggestion);
    await flush();
    expect(areaRows()).toHaveLength(1);

    expect(press(suggestion, "Delete")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(0);
    expect(viewport()).toHaveFocus();
  });

  it("removes the selected area while the view has the focus, and nothing when none is selected", async () => {
    await openWithAll();
    await drawAreas(2);
    viewport().focus();
    press(viewport(), "Backspace");
    await flush();
    expect(areaRows()).toHaveLength(2);

    fireEvent.click(areaElements()[1]);
    viewport().focus();
    press(viewport(), "Backspace");
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(viewport()).toHaveFocus();
  });

  it("removes the area of the row that has the focus and keeps the focus in the list", async () => {
    await openWithAll();
    await drawAreas(2);
    fireEvent.click(areaElements()[0]);
    const [, secondRow] = areaRows();
    const list = secondRow.closest(".crop-area-list");
    const check = secondRow.querySelector("input");
    if (check === null) {
      throw new Error("the row has no checkbox");
    }
    check.focus();

    expect(press(check, "Delete")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(1);
    // The selected area was the other one, and stays.
    expect(areaElements()[0]).toHaveAttribute("data-selected", "true");
    expect(list).toHaveFocus();
  });

  it("leaves the areas alone outside the page and the list", async () => {
    await openWithAll();
    await drawAreas(1);
    fireEvent.click(areaElements()[0]);
    const outside = viewMenuButton();
    outside.focus();
    press(outside, "Backspace");
    await flush();
    expect(areaRows()).toHaveLength(1);
  });

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

    // Nor from elsewhere on the page, or from the list.
    const suggestion = screen.getByTestId("crop-candidate");
    suggestion.focus();
    expect(press(suggestion, "Backspace")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(suggestion).toHaveFocus();

    // The row's checkbox is disabled then; its button is not.
    const show = areaRows()[0].querySelector<HTMLElement>(".crop-area-main");
    if (show === null) {
      throw new Error("the row has no button");
    }
    show.focus();
    expect(press(show, "Delete")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(show).toHaveFocus();
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
