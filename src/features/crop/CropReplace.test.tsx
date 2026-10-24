import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CROP_DROPPED_EVENT,
  type CropDropped,
  type CropPdf,
  type IpcError,
} from "../../ipc";
import {
  areaRows,
  brokenCropPdf,
  callsOf,
  cropPdf,
  emitJobFinished,
  emitJobItem,
  drawWholePage,
  expectPageShown,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  type Call,
} from "./cropTestSupport";

const PASSWORD: IpcError = { code: "PasswordProtected", detail: null };
const BROKEN: IpcError = { code: "PdfOpenFailed", detail: null };

const NEXT = cropPdf({ id: 8, name: "next.pdf", pageCount: 6 });

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

/** What the file dialog picks next. */
let picked: CropPdf;

async function openWithAreas(count = 2): Promise<Call[]> {
  picked = NEXT;
  let first = true;
  const calls = await openPdfInCropTab(
    {
      open_crop_pdf: () => {
        if (first) {
          first = false;
          return cropPdf();
        }
        return picked;
      },
      pick_output_dir: () => ({ dirLabel: "figures" }),
    },
    cropPdf(),
  );
  for (let page = 1; page <= count; page += 1) {
    fireEvent.click(
      screen.getByRole("button", { name: new RegExp(`^ページ ${page}(、|$)`) }),
    );
    await flush();
    await drawWholePage();
  }
  return calls;
}

async function pressOpenAnother() {
  fireEvent.click(screen.getByRole("button", { name: "別の PDF を開く" }));
  await flush();
}

async function drop(payload: Partial<CropDropped>) {
  await act(async () => {
    await emit(CROP_DROPPED_EVENT, {
      cropPdf: null,
      notOnePdf: false,
      error: null,
      ...payload,
    });
  });
}

function dialog(): HTMLElement {
  return screen.getByRole("dialog", {
    name: "書き出していない範囲を破棄して開きますか？",
  });
}

describe("replacing a PDF whose areas were not exported", () => {
  it("asks first, with the name of the file and how many areas would be lost", async () => {
    const calls = await openWithAreas(2);
    await pressOpenAnother();

    const card = dialog();
    expect(within(card).getByText("next.pdf")).toBeInTheDocument();
    expect(
      within(card).getByText("書き出していない範囲が 2 個あります"),
    ).toBeInTheDocument();
    expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
      { id: 7 },
    ]);
    expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(0);
    expect(areaRows()).toHaveLength(2);
  });

  it("has the buttons at the right, and the focus on cancelling", async () => {
    await openWithAreas(1);
    await pressOpenAnother();

    const buttons = within(dialog()).getAllByRole("button");
    expect(buttons.map((b) => b.textContent)).toEqual([
      "キャンセル",
      "破棄して開く",
    ]);
    expect(screen.getByRole("button", { name: "キャンセル" })).toHaveFocus();
  });

  it("opens the other PDF and drops the areas on 'discard and open'", async () => {
    const calls = await openWithAreas(2);
    await pressOpenAnother();
    fireEvent.click(screen.getByRole("button", { name: "破棄して開く" }));
    await flush();

    expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
      { id: 7 },
      { id: 8 },
    ]);
    expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("next.pdf")).toBeInTheDocument();
    expectPageShown(1, 6);
    expect(screen.getByText("範囲はまだありません")).toBeInTheDocument();
  });

  it("drops the pending PDF and keeps everything on cancelling, with the focus back on the button", async () => {
    const calls = await openWithAreas(2);
    await pressOpenAnother();
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();

    expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
      { id: 8 },
    ]);
    expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(1);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("report.pdf")).toBeInTheDocument();
    expect(areaRows()).toHaveLength(2);
    expect(
      screen.getByRole("button", { name: "別の PDF を開く" }),
    ).toHaveFocus();
  });

  it("cancels on Esc", async () => {
    const calls = await openWithAreas(1);
    await pressOpenAnother();
    fireEvent.keyDown(window, { key: "Escape" });
    await flush();

    expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
      { id: 8 },
    ]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(areaRows()).toHaveLength(1);
  });

  it("does not ask when every area was exported, but when one was not, counting only that", async () => {
    const calls = await openWithAreas(2);
    // Page 2 is shown, so its area has the key 2.
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    await emitJobItem({ id: 2, status: "ok", outputs: ["a.png"] });
    await emitJobItem({ id: 1, status: "failed", outputs: [] });
    await emitJobFinished({
      succeeded: 1,
      failed: 1,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });

    await pressOpenAnother();
    expect(
      within(dialog()).getByText("書き出していない範囲が 1 個あります"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();

    // The one that failed is written the next time; then nothing is left.
    fireEvent.click(screen.getByRole("button", { name: "書き出す" }));
    await flush();
    await emitJobItem({ id: 1, status: "ok", outputs: ["b.png"] });
    await emitJobFinished({
      succeeded: 2,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    await pressOpenAnother();

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
      { id: 7 },
      { id: 8 },
    ]);
    expect(areaRows()).toHaveLength(0);
  });

  it("counts the areas that are not checked", async () => {
    await openWithAreas(2);
    for (const box of screen.getAllByRole("checkbox", {
      name: /を書き出す$/,
    })) {
      fireEvent.click(box);
    }
    await pressOpenAnother();
    expect(
      within(dialog()).getByText("書き出していない範囲が 2 個あります"),
    ).toBeInTheDocument();
  });

  it("does not ask when there are no areas", async () => {
    picked = NEXT;
    let first = true;
    const calls = await openPdfInCropTab({
      open_crop_pdf: () => {
        if (first) {
          first = false;
          return cropPdf();
        }
        return picked;
      },
    });
    await pressOpenAnother();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(2);
  });

  it("says the count in the singular in English", async () => {
    picked = NEXT;
    let first = true;
    await openPdfInCropTab(
      {
        open_crop_pdf: () => {
          if (first) {
            first = false;
            return cropPdf();
          }
          return picked;
        },
      },
      cropPdf(),
      "en",
    );
    await drawWholePage();
    fireEvent.click(screen.getByRole("button", { name: "Open another PDF" }));
    await flush();

    const card = screen.getByRole("dialog", {
      name: "Discard areas that have not been exported and open?",
    });
    expect(
      within(card).getByText("1 area has not been exported"),
    ).toBeInTheDocument();
    expect(
      within(card).getByRole("button", { name: "Discard and open" }),
    ).toBeInTheDocument();
  });
});

describe("a pending PDF that needs a password", () => {
  it("asks too, and then asks for the password once discarded and opened", async () => {
    const calls = await openWithAreas(1);
    picked = brokenCropPdf(9, PASSWORD, "locked.pdf");
    await pressOpenAnother();

    expect(within(dialog()).getByText("locked.pdf")).toBeInTheDocument();
    expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "破棄して開く" }));
    await flush();

    expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
      { id: 7 },
      { id: 9 },
    ]);
    const password = screen.getByRole("dialog");
    expect(within(password).getByLabelText("パスワード")).toBeInTheDocument();
  });
});

describe("a pending PDF that is broken", () => {
  it("is dropped with its error shown, without asking, and the areas stay", async () => {
    const calls = await openWithAreas(2);
    picked = brokenCropPdf(9, BROKEN);
    await pressOpenAnother();

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
      { id: 9 },
    ]);
    expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(1);
    expect(
      screen.getByText(
        "PDF を読み込めませんでした。ファイルが壊れている可能性があります",
      ),
    ).toBeInTheDocument();
    expect(areaRows()).toHaveLength(2);
  });
});

describe("a PDF dropped while the question is open", () => {
  it("replaces the one asked about, which Rust has dropped already", async () => {
    const calls = await openWithAreas(2);
    await pressOpenAnother();
    expect(within(dialog()).getByText("next.pdf")).toBeInTheDocument();

    await drop({ cropPdf: cropPdf({ id: 12, name: "third.pdf" }) });

    expect(within(dialog()).getByText("third.pdf")).toBeInTheDocument();
    expect(within(dialog()).queryByText("next.pdf")).toBeNull();
    expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "破棄して開く" }));
    await flush();
    expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
      { id: 7 },
      { id: 12 },
    ]);
    expect(screen.getByText("third.pdf")).toBeInTheDocument();
  });

  it("is the one that cancelling drops", async () => {
    const calls = await openWithAreas(1);
    await pressOpenAnother();
    await drop({ cropPdf: cropPdf({ id: 12, name: "third.pdf" }) });
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();
    expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
      { id: 12 },
    ]);
  });

  it("closes the question when it is broken, drops it and shows the error", async () => {
    const calls = await openWithAreas(2);
    await pressOpenAnother();
    await drop({ cropPdf: brokenCropPdf(13, BROKEN) });

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
      { id: 13 },
    ]);
    expect(
      screen.getByText(
        "PDF を読み込めませんでした。ファイルが壊れている可能性があります",
      ),
    ).toBeInTheDocument();
    expect(areaRows()).toHaveLength(2);
    expect(screen.getByText("report.pdf")).toBeInTheDocument();
  });
});

describe("a PDF dropped on the page", () => {
  it("asks the same, and the focus goes to the page on cancelling", async () => {
    const calls = await openWithAreas(1);
    await drop({ cropPdf: cropPdf({ id: 12, name: "dropped.pdf" }) });
    expect(within(dialog()).getByText("dropped.pdf")).toBeInTheDocument();
    expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();

    expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
      { id: 12 },
    ]);
    expect(screen.getByTestId("crop-viewport")).toHaveFocus();
  });

  it("opens at once when nothing would be lost", async () => {
    const calls = await openPdfInCropTab();
    await drop({ cropPdf: cropPdf({ id: 12, name: "dropped.pdf" }) });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
      { id: 7 },
      { id: 12 },
    ]);
  });
});
