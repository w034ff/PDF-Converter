import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { SETTINGS_SAVE_DEBOUNCE_MS } from "./features/settings/useSettingsAutoSave";
import {
  JOB_FINISHED_EVENT,
  JOB_PROGRESS_EVENT,
  type AddResult,
  type ImageItem,
  type JobFinishedPayload,
  type JobProgressPayload,
  type Settings,
} from "./ipc";

// The app listens for job events on mount, so events are mocked too.
function mockAppIpc(handler: Parameters<typeof mockIPC>[0]) {
  mockIPC(handler, { shouldMockEvents: true });
}

const SAVED_SETTINGS: Settings = {
  language: "en",
  imagesToPdf: {
    output: "each",
    pageSize: "a4",
    outputDir: { dirLabel: "Scans" },
  },
  pdfToImages: {
    format: "jpeg",
    dpi: 300,
    outputDir: { dirLabel: "Rendered" },
  },
};

const ABOUT_READY = {
  version: "0.1.0",
  pdfiumVersion: "chromium/8076",
  pdfiumReady: true,
  pdfiumError: null,
};

/** Lets the `get_settings` answer and the renders after it through. */
async function flush() {
  await act(async () => {});
}

describe("App", () => {
  // Unmount first: unmounting unlistens, which needs the mocked events.
  afterEach(() => {
    cleanup();
    clearMocks();
    vi.useRealTimers();
  });

  it("shows the app name", () => {
    mockAppIpc(() => new Promise(() => {}));
    render(<App initialSettings={null} />);
    expect(
      screen.getByRole("heading", { name: "PDF Converter" }),
    ).toBeInTheDocument();
  });

  it("switches tabs and updates aria-selected attributes", () => {
    mockAppIpc(() => new Promise(() => {}));
    render(<App initialNavLang="ja" initialSettings={null} />);

    const imagesTab = screen.getByRole("tab", { name: "画像 → PDF" });
    const pdfsTab = screen.getByRole("tab", { name: "PDF → 画像" });

    // Initially images tab is active (tabs use aria-selected, not aria-pressed)
    expect(imagesTab).toHaveAttribute("aria-selected", "true");
    expect(imagesTab).not.toHaveAttribute("aria-pressed");
    expect(pdfsTab).toHaveAttribute("aria-selected", "false");
    expect(pdfsTab).not.toHaveAttribute("aria-pressed");
    expect(
      screen.getByRole("button", { name: "PDF を保存" }),
    ).toBeInTheDocument();

    // Click PDF to Images tab
    fireEvent.click(pdfsTab);

    expect(imagesTab).toHaveAttribute("aria-selected", "false");
    expect(imagesTab).not.toHaveAttribute("aria-pressed");
    expect(pdfsTab).toHaveAttribute("aria-selected", "true");
    expect(pdfsTab).not.toHaveAttribute("aria-pressed");
    expect(
      screen.getByRole("button", { name: "変換を開始" }),
    ).toBeInTheDocument();
  });

  it("switches UI language on language select change", () => {
    mockAppIpc(() => new Promise(() => {}));
    render(<App initialNavLang="ja" initialSettings={null} />);

    const select = screen.getByRole("combobox", { name: "言語" });
    expect(select).toHaveValue("ja");
    expect(screen.getByRole("tab", { name: "画像 → PDF" })).toBeInTheDocument();

    // Switch to English
    fireEvent.change(select, { target: { value: "en" } });

    expect(select).toHaveValue("en");
    expect(select).toHaveAccessibleName("Language");
    expect(
      screen.getByRole("tab", { name: "Images → PDF" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "PDF → Images" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save PDF" }),
    ).toBeInTheDocument();
  });

  it("initializes with English when initialNavLang is en-US", () => {
    mockAppIpc(() => new Promise(() => {}));
    render(<App initialNavLang="en-US" initialSettings={null} />);

    expect(
      screen.getByRole("tab", { name: "Images → PDF" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save PDF" }),
    ).toBeInTheDocument();
  });

  describe("restoring the settings", () => {
    it("starts in the saved language and settings", async () => {
      mockAppIpc((cmd) => (cmd === "get_settings" ? SAVED_SETTINGS : null));
      render(<App initialNavLang="ja" />);
      await flush();

      // The saved language wins over the OS language.
      expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue(
        "en",
      );
      const output = screen.getByRole("group", { name: "Output" });
      expect(
        within(output).getByRole("button", { name: "Each image" }),
      ).toHaveAttribute("aria-pressed", "true");
      const pageSize = screen.getByRole("group", { name: "Page size" });
      expect(
        within(pageSize).getByRole("button", { name: "A4" }),
      ).toHaveAttribute("aria-pressed", "true");
      expect(screen.getByText("Scans")).toBeInTheDocument();

      fireEvent.click(screen.getByRole("tab", { name: "PDF → Images" }));
      const format = screen.getByRole("group", { name: "Format" });
      expect(
        within(format).getByRole("button", { name: "JPEG" }),
      ).toHaveAttribute("aria-pressed", "true");
      expect(screen.getByRole("combobox", { name: "Resolution" })).toHaveValue(
        "300",
      );
      expect(screen.getByText("Rendered")).toBeInTheDocument();
    });

    it("follows the OS language when none was saved", async () => {
      mockAppIpc((cmd) =>
        cmd === "get_settings" ? { ...SAVED_SETTINGS, language: null } : null,
      );
      render(<App initialNavLang="ja-JP" />);
      await flush();

      expect(screen.getByRole("combobox", { name: "言語" })).toHaveValue("ja");
    });

    it("shows nothing until the settings are read", () => {
      mockAppIpc(() => new Promise(() => {}));
      render(<App initialNavLang="ja" />);

      expect(
        screen.queryByRole("heading", { name: "PDF Converter" }),
      ).not.toBeInTheDocument();
    });

    it("starts from the defaults when the settings cannot be read", async () => {
      mockAppIpc((cmd) => {
        if (cmd === "get_settings") {
          throw { code: "ReadFailed", detail: null };
        }
        return null;
      });
      render(<App initialNavLang="ja" />);
      await flush();

      const output = screen.getByRole("group", { name: "出力" });
      expect(
        within(output).getByRole("button", { name: "1 つの PDF" }),
      ).toHaveAttribute("aria-pressed", "true");
    });

    it("starts from the defaults when the answer has the wrong shape", async () => {
      mockAppIpc((cmd) =>
        cmd === "get_settings"
          ? { ...SAVED_SETTINGS, imagesToPdf: { output: "each" } }
          : null,
      );
      render(<App initialNavLang="ja" />);
      await flush();

      const output = screen.getByRole("group", { name: "出力" });
      expect(
        within(output).getByRole("button", { name: "1 つの PDF" }),
      ).toHaveAttribute("aria-pressed", "true");
    });
  });

  describe("saving the settings", () => {
    function mockSaves() {
      const saved: unknown[] = [];
      mockAppIpc((cmd, args) => {
        if (cmd === "save_settings") {
          saved.push(args);
          return null;
        }
        return new Promise(() => {});
      });
      return saved;
    }

    it("saves a run of changes once, after they settle", async () => {
      vi.useFakeTimers();
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={null} />);
      fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));

      const dpi = screen.getByRole("combobox", { name: "解像度" });
      fireEvent.change(dpi, { target: { value: "72" } });
      act(() => {
        vi.advanceTimersByTime(SETTINGS_SAVE_DEBOUNCE_MS - 1);
      });
      fireEvent.change(dpi, { target: { value: "300" } });
      fireEvent.click(screen.getByRole("button", { name: "JPEG" }));
      act(() => {
        vi.advanceTimersByTime(SETTINGS_SAVE_DEBOUNCE_MS - 1);
      });
      expect(saved).toEqual([]);

      await act(async () => {
        vi.advanceTimersByTime(1);
      });
      expect(saved).toEqual([
        {
          settings: {
            language: null,
            imagesToPdf: { output: "merge", pageSize: "fit" },
            pdfToImages: { format: "jpeg", dpi: 300 },
          },
        },
      ]);
    });

    it("does not save what the app started with", async () => {
      vi.useFakeTimers();
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={SAVED_SETTINGS} />);

      await act(async () => {
        vi.advanceTimersByTime(SETTINGS_SAVE_DEBOUNCE_MS * 2);
      });
      expect(saved).toEqual([]);
    });

    it("does not save a change that was undone before the save", async () => {
      vi.useFakeTimers();
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={null} />);

      const pageSize = screen.getByRole("group", { name: "ページの大きさ" });
      fireEvent.click(within(pageSize).getByRole("button", { name: "A4" }));
      fireEvent.click(
        within(pageSize).getByRole("button", { name: "画像に合わせる" }),
      );
      await act(async () => {
        vi.advanceTimersByTime(SETTINGS_SAVE_DEBOUNCE_MS * 2);
      });
      expect(saved).toEqual([]);
    });

    it("saves the picked language, not the one taken from the OS", async () => {
      vi.useFakeTimers();
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={null} />);

      fireEvent.change(screen.getByRole("combobox", { name: "言語" }), {
        target: { value: "en" },
      });
      await act(async () => {
        vi.advanceTimersByTime(SETTINGS_SAVE_DEBOUNCE_MS);
      });
      expect(saved).toEqual([
        {
          settings: {
            language: "en",
            imagesToPdf: { output: "merge", pageSize: "fit" },
            pdfToImages: { format: "png", dpi: 150 },
          },
        },
      ]);
    });
  });

  describe("about this app", () => {
    beforeAll(async () => {
      // The dialog loads the license list on demand; load it once up front
      // so the first test that expands it does not time out.
      await import("./licenses");
    });

    function openAbout() {
      fireEvent.click(
        screen.getByRole("button", { name: "このアプリについて" }),
      );
      return screen.getByRole("dialog", { name: "このアプリについて" });
    }

    it("shows the version, pdfium and this app's license", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? ABOUT_READY : null));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const dialog = openAbout();

      expect(
        await within(dialog).findByText("バージョン 0.1.0"),
      ).toBeInTheDocument();
      expect(
        within(dialog).getByText("PDF の描画エンジン: pdfium chromium/8076"),
      ).toBeInTheDocument();
      expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
      expect(
        within(dialog).getByText(/Permission is hereby granted/),
      ).toBeInTheDocument();
    });

    it("says why pdfium does not load", async () => {
      mockAppIpc((cmd) =>
        cmd === "get_about"
          ? {
              ...ABOUT_READY,
              pdfiumReady: false,
              pdfiumError: "PdfiumUnavailable: library not found",
            }
          : null,
      );
      render(<App initialNavLang="ja" initialSettings={null} />);
      const dialog = openAbout();

      expect(await within(dialog).findByRole("alert")).toHaveTextContent(
        "pdfium を読み込めません: PdfiumUnavailable: library not found",
      );
    });

    it("says so when the version cannot be read", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? { version: 1 } : null));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const dialog = openAbout();

      expect(
        await within(dialog).findByText("バージョンを取得できませんでした"),
      ).toBeInTheDocument();
    });

    it("lists the third-party licenses, pdfium included", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? ABOUT_READY : null));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const dialog = openAbout();

      const toggle = within(dialog).getByRole("button", {
        name: "第三者ライセンスを表示",
      });
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      fireEvent.click(toggle);
      expect(toggle).toHaveAttribute("aria-expanded", "true");

      expect(
        await within(dialog).findByText("pdfium-binaries chromium/8076"),
      ).toBeInTheDocument();
      expect(
        within(dialog).getByText("pdfium/freetype.txt chromium/8076"),
      ).toBeInTheDocument();
      expect(within(dialog).getAllByText(/^tauri /).length).toBeGreaterThan(0);
      expect(within(dialog).getAllByText(/^react /).length).toBeGreaterThan(0);
    });

    it("moves focus in, keeps the app inert, and gives focus back on Escape", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? ABOUT_READY : null));
      const { container } = render(
        <App initialNavLang="ja" initialSettings={null} />,
      );
      const aboutButton = screen.getByRole("button", {
        name: "このアプリについて",
      });
      const dialog = openAbout();
      await within(dialog).findByText("バージョン 0.1.0");

      expect(dialog.contains(document.activeElement)).toBe(true);
      const shell = container.querySelector(".app-container");
      expect(shell).toHaveAttribute("inert");

      fireEvent.keyDown(window, { key: "Escape" });

      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(shell).not.toHaveAttribute("inert");
      expect(aboutButton).toHaveFocus();
    });

    it("closes with the close button at the bottom", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? ABOUT_READY : null));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const dialog = openAbout();
      await within(dialog).findByText("バージョン 0.1.0");

      const closeButtons = within(dialog).getAllByRole("button", {
        name: "閉じる",
      });
      fireEvent.click(closeButtons[closeButtons.length - 1]);

      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  describe("footer status", () => {
    it("shows idle status according to active tab and language", () => {
      mockAppIpc(() => new Promise(() => {}));
      render(<App initialNavLang="ja" initialSettings={null} />);

      // On Images tab initially
      expect(screen.getByText("画像が選ばれていません")).toBeInTheDocument();

      // Switch to PDF to Images tab
      fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
      expect(screen.getByText("PDF が選ばれていません")).toBeInTheDocument();
      expect(
        screen.queryByText("画像が選ばれていません"),
      ).not.toBeInTheDocument();

      // Switch language to English
      const select = screen.getByRole("combobox", { name: "言語" });
      fireEvent.change(select, { target: { value: "en" } });
      expect(screen.getByText("No PDFs selected")).toBeInTheDocument();

      // Switch back to Images tab in English
      fireEvent.click(screen.getByRole("tab", { name: "Images → PDF" }));
      expect(screen.getByText("No images selected")).toBeInTheDocument();
    });

    it("prioritizes job progress and finished state over idle status", async () => {
      const sampleItem1: ImageItem = {
        id: 1,
        name: "test1.png",
        width: 100,
        height: 100,
        format: "png",
        bytes: 1000,
        error: null,
      };
      const sampleItem2: ImageItem = {
        id: 2,
        name: "test2.png",
        width: 100,
        height: 100,
        format: "png",
        bytes: 1000,
        error: null,
      };

      mockAppIpc((cmd) => {
        if (cmd === "add_images") {
          const result: AddResult<ImageItem> = {
            added: [sampleItem1, sampleItem2],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return result;
        }
        if (cmd === "save_merged_pdf") {
          return { savedName: "merged.pdf" };
        }
        return undefined;
      });

      render(<App initialNavLang="ja" initialSettings={null} />);

      // Add images via dialog
      fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
      await screen.findByText("test1.png");

      // Footer now shows the idle status for 2 images
      expect(screen.getByText("2 ページの PDF になります")).toBeInTheDocument();

      // Click "PDF を保存" to start conversion
      fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));

      // While job runs (e.g. progress event received), JobFooter shows progress instead of idle status
      const progressPayload: JobProgressPayload = {
        done: 1,
        total: 2,
        current: "test1.png",
      };
      await act(async () => {
        await emit(JOB_PROGRESS_EVENT, progressPayload);
      });

      expect(
        screen.queryByText("2 ページの PDF になります"),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("progressbar", { name: "進捗" }),
      ).toBeInTheDocument();

      // When job finishes, JobFooter shows "完了" and idle status is still not shown
      const finishedPayload: JobFinishedPayload = {
        succeeded: 2,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      };
      await act(async () => {
        await emit(JOB_FINISHED_EVENT, finishedPayload);
      });

      expect(screen.getByText("完了")).toBeInTheDocument();
      expect(
        screen.queryByText("2 ページの PDF になります"),
      ).not.toBeInTheDocument();
    });
  });
});
