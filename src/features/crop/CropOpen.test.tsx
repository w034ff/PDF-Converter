import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import {
  CROP_DROPPED_EVENT,
  type CropDropped,
  type CropPdf,
  type IpcError,
} from "../../ipc";
import {
  brokenCropPdf,
  callsOf,
  cropPdf,
  expectPageShown,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  liesUnderToolbar,
  mockCropIpc,
  openCropTab,
  openPdfInCropTab,
} from "./cropTestSupport";

const PASSWORD: IpcError = { code: "PasswordProtected", detail: null };
const BROKEN: IpcError = { code: "PdfOpenFailed", detail: null };

function renderApp() {
  render(<App initialNavLang="ja" initialSettings={null} />);
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

function pressOpen(name: string) {
  fireEvent.click(screen.getByRole("button", { name }));
}

describe("the crop tab: opening a PDF", () => {
  beforeEach(() => {
    installFakeResizeObserver(600, 800);
    installFakeObjectUrls();
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    vi.unstubAllGlobals();
  });

  describe("the tab", () => {
    it("is the third tab and shows the crop screen", async () => {
      mockCropIpc();
      renderApp();

      const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
      expect(tabs).toEqual(["画像 → PDF", "PDF → 画像", "切り出し"]);

      openCropTab();
      await flush();

      expect(screen.getByRole("tab", { name: "切り出し" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      expect(screen.queryByRole("button", { name: "PDF を保存" })).toBeNull();
      expect(
        screen.getByText(
          "PDF をドロップするか、「PDF を開く」を押してください",
        ),
      ).toBeInTheDocument();
    });

    it("is named Crop in English", () => {
      mockCropIpc();
      render(<App initialNavLang="en" initialSettings={null} />);
      expect(screen.getByRole("tab", { name: "Crop" })).toBeInTheDocument();
    });

    it("has no settings panel: the thumbnails, the page and the areas take its place", async () => {
      mockCropIpc({ open_crop_pdf: () => cropPdf() });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(document.querySelector(".app-sidebar")).toBeNull();
      expect(
        screen.getByRole("navigation", { name: "ページ" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("region", { name: "範囲と書き出しの設定" }),
      ).toBeInTheDocument();
    });

    it("leaves the other tabs as they were", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
      expect(
        screen.getByRole("button", { name: "変換を開始" }),
      ).toBeInTheDocument();
      expect(screen.getByRole("group", { name: "形式" })).toBeInTheDocument();

      fireEvent.click(screen.getByRole("tab", { name: "画像 → PDF" }));
      expect(
        screen.getByRole("button", { name: "PDF を保存" }),
      ).toBeInTheDocument();
    });
  });

  describe("telling Rust where a drop goes", () => {
    it("says lists at the start, crop on the crop tab and lists on leaving it", async () => {
      const calls = mockCropIpc();
      renderApp();
      await flush();
      expect(callsOf(calls, "set_drop_target").map((c) => c.args)).toEqual([
        { target: "lists" },
      ]);

      openCropTab();
      await flush();
      expect(callsOf(calls, "set_drop_target").map((c) => c.args)).toEqual([
        { target: "lists" },
        { target: "crop" },
      ]);

      fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
      await flush();
      expect(callsOf(calls, "set_drop_target").map((c) => c.args)).toEqual([
        { target: "lists" },
        { target: "crop" },
        { target: "lists" },
      ]);

      openCropTab();
      fireEvent.click(screen.getByRole("tab", { name: "画像 → PDF" }));
      await flush();
      expect(callsOf(calls, "set_drop_target").map((c) => c.args)).toEqual([
        { target: "lists" },
        { target: "crop" },
        { target: "lists" },
        { target: "crop" },
        { target: "lists" },
      ]);
    });

    it("carries on when Rust refuses", async () => {
      mockCropIpc({
        set_drop_target: () => {
          throw { code: "InvalidParams", detail: null };
        },
      });
      renderApp();
      openCropTab();
      await flush();
      expect(
        screen.getByRole("button", { name: "PDF を開く" }),
      ).toBeInTheDocument();
    });
  });

  describe("the empty screen", () => {
    it("asks to drop or open a PDF, explains and offers the button", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      expect(
        screen.getByText(
          "ページの図・グラフ・表・写真を選んで、画像として書き出します。写真は元の画素のまま書き出せます。",
        ),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "PDF を開く" })).toBeEnabled();
      expect(screen.queryByRole("navigation", { name: "ページ" })).toBeNull();
      expect(
        screen.queryByRole("region", { name: "範囲と書き出しの設定" }),
      ).toBeNull();
    });

    it("says in the bottom bar that no PDF is open, with Export unavailable", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      expect(screen.getByText("PDF が開かれていません")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "書き出す" })).toBeDisabled();
    });

    it("is in English too", async () => {
      mockCropIpc();
      render(<App initialNavLang="en" initialSettings={null} />);
      openCropTab("Crop");
      await flush();

      expect(
        screen.getByText('Drop a PDF here or press "Open PDF"'),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Open PDF" }),
      ).toBeInTheDocument();
      expect(screen.getByText("No PDF is open")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Export" })).toBeDisabled();
    });
  });

  describe("opening with the dialog", () => {
    it("changes nothing when the dialog is cancelled", async () => {
      const calls = mockCropIpc({ open_crop_pdf: () => null });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(callsOf(calls, "open_crop_pdf")).toHaveLength(1);
      expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(0);
      expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(0);
      expect(
        screen.getByRole("button", { name: "PDF を開く" }),
      ).toBeInTheDocument();
      expect(screen.queryByRole("alert")).toBeNull();
    });

    it("activates a PDF that opened and shows its name and page", async () => {
      const calls = mockCropIpc({
        open_crop_pdf: () =>
          cropPdf({ id: 7, name: "market-report.pdf", pageCount: 12 }),
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
        { id: 7 },
      ]);
      expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(0);
      // The name and the button sit at the left of the toolbar; the name
      // shows whole on hover, as it may be cut short.
      const toolbar = screen.getByText("market-report.pdf").parentElement;
      expect(screen.getByText("market-report.pdf")).toHaveAttribute(
        "title",
        "market-report.pdf",
      );
      expectPageShown(1, 12);
      const openAnother = screen.getByRole("button", {
        name: "別の PDF を開く",
      });
      expect(openAnother).toHaveAttribute("title", "別の PDF を開く");
      expect(toolbar).toContainElement(openAnother);
      expect(screen.getByText("範囲が選ばれていません")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "書き出す" })).toBeDisabled();
    });

    it("opens another PDF with the button over the page", async () => {
      let opened = 0;
      const calls = mockCropIpc({
        open_crop_pdf: () => {
          opened += 1;
          return cropPdf({ id: 7 + opened, name: `file-${opened}.pdf` });
        },
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();
      expect(screen.getByText("file-1.pdf")).toBeInTheDocument();

      pressOpen("別の PDF を開く");
      await flush();

      expect(screen.getByText("file-2.pdf")).toBeInTheDocument();
      expect(screen.queryByText("file-1.pdf")).toBeNull();
      expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
        { id: 8 },
        { id: 9 },
      ]);
    });

    it("drops a broken file, keeps the open PDF and says what went wrong", async () => {
      let opened = 0;
      const calls = mockCropIpc({
        open_crop_pdf: () => {
          opened += 1;
          return opened === 1
            ? cropPdf({ id: 7, name: "good.pdf" })
            : brokenCropPdf(8, BROKEN);
        },
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      pressOpen("別の PDF を開く");
      await flush();

      expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
        { id: 8 },
      ]);
      expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
        { id: 7 },
      ]);
      expect(screen.getByText("good.pdf")).toBeInTheDocument();
      expectPageShown(1, 4);
      expect(screen.getByRole("alert")).toHaveTextContent(
        "PDF を読み込めませんでした。ファイルが壊れている可能性があります",
      );
    });

    it("says what went wrong with a broken file when nothing was open", async () => {
      const calls = mockCropIpc({
        open_crop_pdf: () => brokenCropPdf(3, BROKEN),
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(1);
      expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(0);
      expect(screen.getByRole("alert")).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "PDF を開く" }),
      ).toBeInTheDocument();
    });

    it("lets the message be dismissed, and clears it when another PDF opens", async () => {
      let opened = 0;
      mockCropIpc({
        open_crop_pdf: () => {
          opened += 1;
          return opened === 1 ? brokenCropPdf(3, BROKEN) : cropPdf({ id: 4 });
        },
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();
      fireEvent.click(screen.getByRole("button", { name: "閉じる" }));
      expect(screen.queryByRole("alert")).toBeNull();

      pressOpen("PDF を開く");
      await flush();
      expect(screen.getByText("report.pdf")).toBeInTheDocument();
    });

    it("shows the error of a command that fails", async () => {
      mockCropIpc({
        open_crop_pdf: () => {
          throw { code: "ConversionRunning", detail: null };
        },
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(screen.getByRole("alert")).toHaveTextContent(
        "変換中は操作できません",
      );
    });

    it("shows the error when the pending PDF can no longer be activated", async () => {
      mockCropIpc({
        open_crop_pdf: () => cropPdf(),
        activate_crop_pdf: () => {
          throw { code: "UnknownHandle", detail: null };
        },
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(screen.getByRole("alert")).toHaveTextContent(
        "ファイルをもう一度追加してください",
      );
      expect(
        screen.getByRole("button", { name: "PDF を開く" }),
      ).toBeInTheDocument();
    });
  });

  describe("a PDF with a password", () => {
    function lockedPdf(): CropPdf {
      return brokenCropPdf(9, PASSWORD, "locked.pdf");
    }

    it("is activated and then asks for the password", async () => {
      const calls = mockCropIpc({ open_crop_pdf: () => lockedPdf() });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
        { id: 9 },
      ]);
      expect(callsOf(calls, "discard_crop_pdf")).toHaveLength(0);
      const dialog = screen.getByRole("dialog");
      expect(within(dialog).getByText("locked.pdf")).toBeInTheDocument();
      expect(within(dialog).getByLabelText("パスワード")).toBeInTheDocument();
    });

    it("keeps the name and the button to open another, without a page", async () => {
      mockCropIpc({ open_crop_pdf: () => lockedPdf() });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();
      fireEvent.click(
        within(screen.getByRole("dialog")).getByRole("button", {
          name: "キャンセル",
        }),
      );
      await flush();

      expect(screen.getByText("locked.pdf")).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "別の PDF を開く" }),
      ).toBeEnabled();
      expect(screen.queryByTestId("crop-viewport")).toBeNull();
    });

    it("keeps the dialog open with an error for a wrong password", async () => {
      const calls = mockCropIpc({
        open_crop_pdf: () => lockedPdf(),
        unlock_crop_pdf: () => {
          throw PASSWORD;
        },
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      const dialog = screen.getByRole("dialog");
      fireEvent.change(within(dialog).getByLabelText("パスワード"), {
        target: { value: "wrong" },
      });
      fireEvent.click(within(dialog).getByRole("button", { name: "開く" }));
      await flush();

      expect(callsOf(calls, "unlock_crop_pdf").map((c) => c.args)).toEqual([
        { id: 9, password: "wrong" },
      ]);
      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(
        within(screen.getByRole("dialog")).getByRole("alert"),
      ).toHaveTextContent("パスワードが違います");
    });

    it("replaces the PDF with the unlocked one for the right password", async () => {
      mockCropIpc({
        open_crop_pdf: () => lockedPdf(),
        unlock_crop_pdf: () =>
          cropPdf({ id: 9, name: "locked.pdf", pageCount: 6 }),
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      const dialog = screen.getByRole("dialog");
      fireEvent.change(within(dialog).getByLabelText("パスワード"), {
        target: { value: "secret" },
      });
      fireEvent.click(within(dialog).getByRole("button", { name: "開く" }));
      await flush();

      expect(screen.queryByRole("dialog")).toBeNull();
      expectPageShown(1, 6);
      expect(
        screen.getByRole("navigation", { name: "ページ" }),
      ).toBeInTheDocument();
      expect(screen.queryByText("パスワードを入力")).toBeNull();
    });

    it("offers to enter the password again after the dialog is cancelled", async () => {
      mockCropIpc({ open_crop_pdf: () => lockedPdf() });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      fireEvent.click(
        within(screen.getByRole("dialog")).getByRole("button", {
          name: "キャンセル",
        }),
      );

      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.getByText("locked.pdf")).toBeInTheDocument();
      expect(
        screen.getByText("パスワードで保護されています"),
      ).toBeInTheDocument();
      expect(screen.queryByRole("navigation", { name: "ページ" })).toBeNull();

      fireEvent.click(screen.getByRole("button", { name: "パスワードを入力" }));
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });

    it("moves the focus to the page once it is unlocked", async () => {
      mockCropIpc({
        open_crop_pdf: () => lockedPdf(),
        unlock_crop_pdf: () => cropPdf({ id: 9, name: "locked.pdf" }),
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();
      fireEvent.click(
        within(screen.getByRole("dialog")).getByRole("button", {
          name: "キャンセル",
        }),
      );
      fireEvent.click(screen.getByRole("button", { name: "パスワードを入力" }));
      const dialog = screen.getByRole("dialog");
      fireEvent.change(within(dialog).getByLabelText("パスワード"), {
        target: { value: "secret" },
      });
      fireEvent.click(within(dialog).getByRole("button", { name: "開く" }));
      await flush();

      expect(document.activeElement).toBe(screen.getByTestId("crop-viewport"));
    });
  });

  describe("a drop", () => {
    it("opens a PDF that was dropped", async () => {
      const calls = mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      await drop({
        cropPdf: cropPdf({ id: 21, name: "dropped.pdf", pageCount: 3 }),
      });

      expect(callsOf(calls, "activate_crop_pdf").map((c) => c.args)).toEqual([
        { id: 21 },
      ]);
      expect(screen.getByText("dropped.pdf")).toBeInTheDocument();
      expectPageShown(1, 3);
    });

    it("drops a broken PDF that was dropped, keeping the open one", async () => {
      const calls = mockCropIpc({
        open_crop_pdf: () => cropPdf({ name: "open.pdf" }),
      });
      renderApp();
      openCropTab();
      pressOpen("PDF を開く");
      await flush();

      await drop({ cropPdf: brokenCropPdf(22, BROKEN) });

      expect(callsOf(calls, "discard_crop_pdf").map((c) => c.args)).toEqual([
        { id: 22 },
      ]);
      expect(screen.getByText("open.pdf")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    it("asks for the password of a PDF that was dropped", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      await drop({ cropPdf: brokenCropPdf(23, PASSWORD, "locked.pdf") });

      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });

    it("says to open one PDF at a time when the drop was anything else", async () => {
      const calls = mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      await drop({ notOnePdf: true });

      expect(screen.getByRole("status")).toHaveTextContent(
        "切り出しでは PDF を 1 つずつ開きます",
      );
      expect(callsOf(calls, "activate_crop_pdf")).toHaveLength(0);
      expect(
        screen.getByRole("button", { name: "PDF を開く" }),
      ).toBeInTheDocument();
    });

    it("says it in English too", async () => {
      mockCropIpc();
      render(<App initialNavLang="en" initialSettings={null} />);
      openCropTab("Crop");
      await flush();

      await drop({ notOnePdf: true });

      expect(screen.getByRole("status")).toHaveTextContent(
        "Open one PDF at a time to crop",
      );
    });

    it("shows the error of a drop during a conversion", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      await drop({ error: { code: "ConversionRunning", detail: null } });

      expect(screen.getByRole("alert")).toHaveTextContent(
        "変換中は操作できません",
      );
    });

    it("keeps the message about a drop when the tab is left and shown again", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();
      await drop({ notOnePdf: true });

      fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
      openCropTab();

      expect(screen.getByRole("status")).toHaveTextContent(
        "切り出しでは PDF を 1 つずつ開きます",
      );
    });

    it("says it under the toolbar while a PDF is open", async () => {
      await openPdfInCropTab();
      await drop({ notOnePdf: true });

      const notice = screen.getByRole("status");
      expect(notice).toHaveTextContent("切り出しでは PDF を 1 つずつ開きます");
      expect(liesUnderToolbar(notice)).toBe(true);
    });

    it("says it under the toolbar while a PDF that needs its password is open", async () => {
      await openPdfInCropTab({}, brokenCropPdf(7, PASSWORD, "locked.pdf"));
      fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
      await drop({ notOnePdf: true });

      const notice = screen
        .getAllByRole("status")
        .find((element) => element.textContent?.includes("1 つずつ"));
      expect(notice).toBeDefined();
      expect(notice !== undefined && liesUnderToolbar(notice)).toBe(true);
    });

    it("does not show a drop for the lists in the crop tab", async () => {
      mockCropIpc();
      renderApp();
      openCropTab();
      await flush();

      await act(async () => {
        await emit("items-dropped", {
          images: [],
          pdfs: [],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          error: { code: "ConversionRunning", detail: null },
        });
      });

      expect(screen.queryByRole("alert")).toBeNull();
    });
  });
});
