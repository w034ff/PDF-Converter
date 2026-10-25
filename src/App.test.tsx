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
    pageSize: "letter",
    orientation: "auto",
    pdfA: false,
    encrypt: false,
    outputDir: { dirLabel: "Scans" },
  },
  pdfToImages: {
    format: "jpeg",
    colorMode: "color",
    dpi: 300,
    outputDir: { dirLabel: "Rendered" },
  },
  crop: {
    format: "png",
    colorMode: "color",
    dpi: 150,
    photoNative: true,
    photoOnly: false,
    outputDir: null,
  },
};

/** What `save_settings` carries for the crop tab until it is touched. */
const DEFAULT_CROP_OPTIONS = {
  format: "png",
  colorMode: "color",
  dpi: 150,
  photoNative: true,
  photoOnly: false,
};

const ABOUT_READY = {
  version: "0.1.0",
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

  describe("browser shortcuts", () => {
    /** Presses `init` on `target` and returns whether the app stopped it. */
    function isStopped(target: EventTarget, init: KeyboardEventInit) {
      const event = new KeyboardEvent("keydown", {
        bubbles: true,
        cancelable: true,
        ...init,
      });
      target.dispatchEvent(event);
      return event.defaultPrevented;
    }

    it("stops find, print and reload, and lets copy and zoom through", () => {
      mockAppIpc(() => new Promise(() => {}));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const target = screen.getByRole("tab", { name: "画像 → PDF" });

      expect(isStopped(target, { key: "f", ctrlKey: true })).toBe(true);
      expect(isStopped(target, { key: "p", ctrlKey: true })).toBe(true);
      expect(isStopped(target, { key: "r", ctrlKey: true })).toBe(true);
      expect(isStopped(target, { key: "F5" })).toBe(true);
      expect(isStopped(target, { key: "c", ctrlKey: true })).toBe(false);
      expect(isStopped(target, { key: "=", ctrlKey: true })).toBe(false);
    });

    it("stops them while a field has focus", () => {
      mockAppIpc(() => new Promise(() => {}));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const field = document.createElement("input");
      document.body.append(field);
      field.focus();

      expect(isStopped(field, { key: "f", ctrlKey: true })).toBe(true);
      expect(isStopped(field, { key: "v", ctrlKey: true })).toBe(false);

      field.remove();
    });
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
      expect(screen.getByRole("combobox", { name: "Page size" })).toHaveValue(
        "letter",
      );
      const orientation = screen.getByRole("group", { name: "Orientation" });
      expect(
        within(orientation).getByRole("button", { name: "Auto" }),
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

    it("starts with the PDF/A box as saved", async () => {
      mockAppIpc((cmd) =>
        cmd === "get_settings"
          ? {
              ...SAVED_SETTINGS,
              imagesToPdf: { ...SAVED_SETTINGS.imagesToPdf, pdfA: true },
            }
          : null,
      );
      render(<App initialNavLang="ja" />);
      await flush();

      expect(
        screen.getByRole("checkbox", { name: "Archival (PDF/A)" }),
      ).toBeChecked();
    });

    it("starts with the password box as saved", async () => {
      mockAppIpc((cmd) =>
        cmd === "get_settings"
          ? {
              ...SAVED_SETTINGS,
              imagesToPdf: { ...SAVED_SETTINGS.imagesToPdf, encrypt: true },
            }
          : null,
      );
      render(<App initialNavLang="ja" />);
      await flush();

      expect(
        screen.getByRole("checkbox", { name: "Password-protect" }),
      ).toBeChecked();
      expect(
        screen.getByRole("checkbox", { name: "Archival (PDF/A)" }),
      ).toBeDisabled();
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

    it("saves each change right away, with a higher revision each time", () => {
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={null} />);
      fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));

      const dpi = screen.getByRole("combobox", { name: "解像度" });
      fireEvent.change(dpi, { target: { value: "300" } });
      fireEvent.click(screen.getByRole("button", { name: "JPEG" }));

      const imagesToPdf = {
        output: "merge",
        pageSize: "fit",
        orientation: "auto",
        pdfA: false,
        encrypt: false,
      };
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: null,
            imagesToPdf,
            pdfToImages: { format: "png", colorMode: "color", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
        {
          revision: 2,
          settings: {
            language: null,
            imagesToPdf,
            pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
      ]);
    });

    it("does not save what the app started with", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={SAVED_SETTINGS} />);

      await act(async () => {});
      expect(saved).toEqual([]);
    });

    it("saves when changing orientation alone", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="en" initialSettings={SAVED_SETTINGS} />);

      const orientation = screen.getByRole("group", { name: "Orientation" });
      fireEvent.click(
        within(orientation).getByRole("button", { name: "Portrait" }),
      );
      await act(async () => {});
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: "en",
            imagesToPdf: {
              output: "each",
              pageSize: "letter",
              orientation: "portrait",
              pdfA: false,
              encrypt: false,
            },
            pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
      ]);
    });

    it("saves when changing the PDF/A choice alone", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="en" initialSettings={SAVED_SETTINGS} />);

      fireEvent.click(
        screen.getByRole("checkbox", { name: "Archival (PDF/A)" }),
      );
      await act(async () => {});
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: "en",
            imagesToPdf: {
              output: "each",
              pageSize: "letter",
              orientation: "auto",
              pdfA: true,
              encrypt: false,
            },
            pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
      ]);
    });

    it("saves when changing the password choice alone", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="en" initialSettings={SAVED_SETTINGS} />);

      fireEvent.click(
        screen.getByRole("checkbox", { name: "Password-protect" }),
      );
      await act(async () => {});
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: "en",
            imagesToPdf: {
              output: "each",
              pageSize: "letter",
              orientation: "auto",
              pdfA: false,
              encrypt: true,
            },
            pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
      ]);
    });

    it("saves when changing page size alone", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="en" initialSettings={SAVED_SETTINGS} />);

      const pageSize = screen.getByRole("combobox", { name: "Page size" });
      fireEvent.change(pageSize, { target: { value: "jisB5" } });
      await act(async () => {});
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: "en",
            imagesToPdf: {
              output: "each",
              pageSize: "jisB5",
              orientation: "auto",
              pdfA: false,
              encrypt: false,
            },
            pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
      ]);
    });

    it("saves when changing colorMode alone", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="en" initialSettings={SAVED_SETTINGS} />);
      fireEvent.click(screen.getByRole("tab", { name: "PDF → Images" }));

      const colorGroup = screen.getByRole("group", { name: "Color" });
      fireEvent.click(
        within(colorGroup).getByRole("button", { name: "Grayscale" }),
      );
      await act(async () => {});
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: "en",
            imagesToPdf: {
              output: "each",
              pageSize: "letter",
              orientation: "auto",
              pdfA: false,
              encrypt: false,
            },
            pdfToImages: { format: "jpeg", colorMode: "gray", dpi: 300 },
            crop: DEFAULT_CROP_OPTIONS,
          },
        },
      ]);
    });

    it("saves the picked language, not the one taken from the OS", async () => {
      const saved = mockSaves();
      render(<App initialNavLang="ja" initialSettings={null} />);

      fireEvent.change(screen.getByRole("combobox", { name: "言語" }), {
        target: { value: "en" },
      });
      await act(async () => {});
      expect(saved).toEqual([
        {
          revision: 1,
          settings: {
            language: "en",
            imagesToPdf: {
              output: "merge",
              pageSize: "fit",
              orientation: "auto",
              pdfA: false,
              encrypt: false,
            },
            pdfToImages: { format: "png", colorMode: "color", dpi: 150 },
            crop: DEFAULT_CROP_OPTIONS,
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

    it("shows the version and this app's license, and no rendering engine line", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? ABOUT_READY : null));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const dialog = openAbout();

      expect(
        await within(dialog).findByText("バージョン 0.1.0"),
      ).toBeInTheDocument();
      // The version of pdfium is in the third-party list instead.
      expect(
        within(dialog).queryByText(/描画エンジン/),
      ).not.toBeInTheDocument();
      expect(
        within(dialog).queryByText(/chromium\/8076/),
      ).not.toBeInTheDocument();
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

    it("closes with the × at the top right, its only close button", async () => {
      mockAppIpc((cmd) => (cmd === "get_about" ? ABOUT_READY : null));
      render(<App initialNavLang="ja" initialSettings={null} />);
      const aboutButton = screen.getByRole("button", {
        name: "このアプリについて",
      });
      const dialog = openAbout();
      await within(dialog).findByText("バージョン 0.1.0");

      fireEvent.click(within(dialog).getByRole("button", { name: "閉じる" }));

      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(aboutButton).toHaveFocus();
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
        pageCount: 1,
        bytes: 1000,
        error: null,
      };
      const sampleItem2: ImageItem = {
        id: 2,
        name: "test2.png",
        width: 100,
        height: 100,
        format: "png",
        pageCount: 1,
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

      // When job finishes, JobFooter shows the outcome and idle status is still not shown
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

      expect(screen.getByRole("status")).toHaveTextContent(
        /^変換が終わりました：/,
      );
      expect(
        screen.queryByText("2 ページの PDF になります"),
      ).not.toBeInTheDocument();
    });
  });
});
