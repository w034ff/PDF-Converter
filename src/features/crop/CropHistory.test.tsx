import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  areaRows,
  boxPt,
  candidate,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  moveTo,
  openSettings,
  overlay,
  pressAt,
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
const TABLE: [number, number, number, number] = [50, 300, 250, 400];

async function openWithSuggestions() {
  return openPdfInCropTab({
    detect_layout: () => [candidate("chart", CHART), candidate("table", TABLE)],
    pick_output_dir: () => ({ dirLabel: "figures" }),
  });
}

function ctrl(target: Element, key: string, shiftKey = false) {
  return fireEvent.keyDown(target, { key, ctrlKey: true, shiftKey });
}

async function addBoth() {
  const [chart, table] = screen.getAllByTestId("crop-candidate");
  fireEvent.click(chart);
  fireEvent.click(table);
  await flush();
}

describe("undoing", () => {
  it("has no buttons; the keys are in the list of keys", async () => {
    await openWithSuggestions();
    expect(screen.queryByRole("button", { name: "元に戻す" })).toBeNull();
    expect(screen.queryByRole("button", { name: "やり直す" })).toBeNull();
  });

  it("is refused while a conversion runs", async () => {
    await openWithSuggestions();
    await addBoth();
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    ctrl(document.body, "z");
    await flush();
    expect(areaRows()).toHaveLength(2);
  });
});

describe("the keys", () => {
  it("undo with Ctrl+Z and redo with Ctrl+Y or Ctrl+Shift+Z, wherever the focus is", async () => {
    await openWithSuggestions();
    await addBoth();

    expect(ctrl(document.body, "z")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(1);
    ctrl(screen.getByTestId("crop-area"), "Z");
    await flush();
    expect(areaRows()).toHaveLength(0);

    ctrl(document.body, "y");
    await flush();
    expect(areaRows()).toHaveLength(1);
    ctrl(document.body, "Z", true);
    await flush();
    expect(areaRows()).toHaveLength(2);
  });

  it("are left to a field that takes text", async () => {
    await openWithSuggestions();
    await addBoth();
    openSettings();
    fireEvent.change(screen.getByRole("combobox", { name: "解像度" }), {
      target: { value: "custom" },
    });
    const entry = screen.getByRole("textbox", { name: "解像度（dpi）" });

    expect(ctrl(entry, "z")).toBe(true);
    await flush();
    expect(areaRows()).toHaveLength(2);

    // A checkbox takes no text, so the key undoes there.
    const check = areaRows()[0].querySelector("input");
    if (check === null) {
      throw new Error("the row has no checkbox");
    }
    ctrl(check, "z");
    await flush();
    expect(areaRows()).toHaveLength(1);
  });

  it("are left alone while a dialog is open", async () => {
    await openWithSuggestions();
    await addBoth();
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    document.body.append(dialog);
    try {
      expect(ctrl(dialog, "z")).toBe(true);
      await flush();
      expect(areaRows()).toHaveLength(2);
    } finally {
      dialog.remove();
    }
  });

  it("wait while a drag is on", async () => {
    await openWithSuggestions();
    await addBoth();
    pressAt(overlay(), [400, 500]);
    moveTo([450, 560]);
    expect(ctrl(document.body, "z")).toBe(false);
    await flush();
    expect(areaRows()).toHaveLength(2);
  });

  it("take back a run of arrow keys on one area in one go", async () => {
    await openWithSuggestions();
    const [chart] = screen.getAllByTestId("crop-candidate");
    fireEvent.click(chart);
    await flush();
    const area = screen.getByTestId("crop-area");
    const before = boxPt(area);
    for (let index = 0; index < 3; index += 1) {
      fireEvent.keyDown(area, { key: "ArrowRight", shiftKey: true });
    }
    await flush();
    expect(boxPt(screen.getByTestId("crop-area"))[0]).toBeCloseTo(
      before[0] + 30,
      3,
    );

    ctrl(document.body, "z");
    await flush();
    expect(boxPt(screen.getByTestId("crop-area"))[0]).toBeCloseTo(before[0], 3);
  });
});
