import { clearMocks } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { ja } from "../../i18n";
import type { Settings } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  initialJobState,
} from "../../state";
import { CropAction } from "./CropAction";
import { CropExportSettings } from "./CropExportSettings";
import { CropStatus } from "./CropStatus";
import {
  DEFAULT_SETTINGS,
  callsOf,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  mockCropIpc,
  openCropTab,
  savedCrops,
} from "./cropTestSupport";
import { JobFooter } from "../job/JobFooter";

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

/**
 * The export settings are in the column on the right, which shows once a PDF
 * is open. They start closed; `open` opens them, as most tests need them.
 */
async function renderApp(settings: Settings | null = null, open = true) {
  render(<App initialNavLang="ja" initialSettings={settings} />);
  await openPdf();
  if (open) {
    fireEvent.click(screen.getByRole("button", { name: /書き出しの設定/ }));
  }
}

async function openPdf(name = "PDF を開く") {
  openCropTab(name === "Open PDF" ? "Crop" : "切り出し");
  fireEvent.click(screen.getByRole("button", { name }));
  await flush();
  await flush();
}

function withCrop(crop: Partial<Settings["crop"]>): Settings {
  return { ...DEFAULT_SETTINGS, crop: { ...DEFAULT_SETTINGS.crop, ...crop } };
}

function savedSettings(calls: ReturnType<typeof mockCropIpc>) {
  return callsOf(calls, "save_settings").map((call) => call.args);
}

const SUMMARY = "PNG · カラー · 150 dpi · 写真は元の画素";

describe("opening and closing", () => {
  it.each([400, 720, 1200])(
    "starts closed in a window %i px high",
    async (height) => {
      mockCropIpc();
      vi.stubGlobal("innerHeight", height);
      await renderApp(null, false);
      expect(
        screen.getByRole("button", { name: /書き出しの設定/ }),
      ).toHaveAttribute("aria-expanded", "false");
    },
  );

  it("opens and closes with the heading, and does not remember it", async () => {
    const calls = mockCropIpc();
    await renderApp(null);
    const toggle = screen.getByRole("button", { name: /書き出しの設定/ });
    expect(screen.getByRole("group", { name: "形式" })).toBeInTheDocument();

    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("group", { name: "形式" })).toBeNull();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("group", { name: "形式" })).toBeInTheDocument();
    expect(callsOf(calls, "save_settings")).toHaveLength(0);
  });

  it("says the settings in one line on the heading while closed", async () => {
    mockCropIpc();
    await renderApp(null, false);

    const toggle = screen.getByRole("button", { name: /書き出しの設定/ });
    expect(within(toggle).getByText(SUMMARY)).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(screen.queryByText(SUMMARY)).toBeNull();
  });

  it("adds photos only to the line, and leaves out the original pixels when they are off", async () => {
    mockCropIpc();
    await renderApp(
      withCrop({
        format: "webp",
        colorMode: "gray",
        dpi: 300,
        photoNative: false,
        photoOnly: true,
      }),
      false,
    );
    expect(
      screen.getByText("WebP · グレースケール · 300 dpi · 写真だけ"),
    ).toBeInTheDocument();
    cleanup();

    await renderApp(withCrop({ photoOnly: true }), false);
    expect(screen.getByText(`${SUMMARY} · 写真だけ`)).toBeInTheDocument();
  });

  it("says the line in English", async () => {
    mockCropIpc();
    render(<App initialNavLang="en" initialSettings={null} />);
    await openPdf("Open PDF");

    expect(
      screen.getByText(
        "PNG · Full color · 150 dpi · Photos at original pixels",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Export settings/ }),
    ).toBeInTheDocument();
  });

  it("follows the choices while closed", async () => {
    mockCropIpc();
    await renderApp(null);
    fireEvent.click(screen.getByRole("button", { name: "JPEG" }));
    fireEvent.click(
      screen.getByRole("checkbox", { name: "写真だけを書き出す" }),
    );
    fireEvent.click(screen.getByRole("button", { name: /書き出しの設定/ }));

    expect(
      screen.getByText(`JPEG · カラー · 150 dpi · 写真は元の画素 · 写真だけ`),
    ).toBeInTheDocument();
  });
});

describe("the fields", () => {
  it("has the format, color, resolution, photo options and folder", async () => {
    mockCropIpc();
    await renderApp(null);

    expect(screen.getByRole("group", { name: "形式" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "色" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "解像度" })).toHaveValue("150");
    const native = screen.getByRole("checkbox", {
      name: "写真は元の画素で書き出す",
    });
    expect(native).toBeChecked();
    expect(
      screen.getByText("解像度によらず、元の画素の数で書き出します"),
    ).toBeInTheDocument();
    const only = screen.getByRole("checkbox", { name: "写真だけを書き出す" });
    expect(only).not.toBeChecked();
    expect(
      screen.getByText("写真に重なった文字や枠を除きます"),
    ).toBeInTheDocument();
    expect(screen.getByText("保存先フォルダ")).toBeInTheDocument();
    expect(screen.getByText("未選択")).toBeInTheDocument();
  });

  it("starts from the saved settings", async () => {
    mockCropIpc();
    await renderApp(
      withCrop({
        format: "jpeg",
        colorMode: "gray",
        dpi: 300,
        photoNative: false,
        photoOnly: true,
        outputDir: { dirLabel: "figures" },
      }),
    );

    expect(screen.getByRole("button", { name: "JPEG" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(
      screen.getByRole("button", { name: "グレースケール" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("combobox", { name: "解像度" })).toHaveValue("300");
    expect(
      screen.getByRole("checkbox", { name: "写真は元の画素で書き出す" }),
    ).not.toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "写真だけを書き出す" }),
    ).toBeChecked();
    expect(screen.getByText("figures")).toBeInTheDocument();
  });

  it("is separate from the settings of PDF to images", async () => {
    mockCropIpc();
    await renderApp(null);
    fireEvent.click(screen.getByRole("button", { name: "WebP" }));

    fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));

    expect(screen.getByRole("button", { name: "PNG" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "WebP" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("picks the crop folder, not the folder of another conversion", async () => {
    const calls = mockCropIpc({
      pick_output_dir: () => ({ dirLabel: "figures" }),
    });
    await renderApp(null);

    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
    await flush();

    expect(callsOf(calls, "pick_output_dir").map((c) => c.args)).toEqual([
      { kind: "crop" },
    ]);
    expect(screen.getByText("figures")).toBeInTheDocument();
  });
});

describe("saving", () => {
  it("does not save what the app started with", async () => {
    const calls = mockCropIpc();
    await renderApp(withCrop({ format: "jpeg" }));
    await flush();

    expect(callsOf(calls, "save_settings")).toHaveLength(0);
  });

  it("sends the crop options with each change, with a higher revision each time", async () => {
    const calls = mockCropIpc();
    await renderApp(null);

    fireEvent.click(screen.getByRole("button", { name: "JPEG" }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "グレースケール" }));
    await flush();
    fireEvent.change(screen.getByRole("combobox", { name: "解像度" }), {
      target: { value: "300" },
    });
    await flush();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "写真は元の画素で書き出す" }),
    );
    await flush();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "写真だけを書き出す" }),
    );
    await flush();

    const saved = callsOf(calls, "save_settings");
    expect(saved.map((c) => c.args.revision)).toEqual([1, 2, 3, 4, 5]);
    expect(savedCrops(calls)).toEqual([
      {
        format: "jpeg",
        colorMode: "color",
        dpi: 150,
        photoNative: true,
        photoOnly: false,
      },
      {
        format: "jpeg",
        colorMode: "gray",
        dpi: 150,
        photoNative: true,
        photoOnly: false,
      },
      {
        format: "jpeg",
        colorMode: "gray",
        dpi: 300,
        photoNative: true,
        photoOnly: false,
      },
      {
        format: "jpeg",
        colorMode: "gray",
        dpi: 300,
        photoNative: false,
        photoOnly: false,
      },
      {
        format: "jpeg",
        colorMode: "gray",
        dpi: 300,
        photoNative: false,
        photoOnly: true,
      },
    ]);
  });

  it("still sends the other tabs' settings along with it", async () => {
    const calls = mockCropIpc();
    await renderApp(null);

    fireEvent.click(screen.getByRole("button", { name: "WebP" }));
    await flush();

    expect(savedSettings(calls)).toEqual([
      {
        revision: 1,
        settings: {
          language: null,
          imagesToPdf: {
            output: "merge",
            pageSize: "fit",
            orientation: "auto",
            pdfA: false,
            encrypt: false,
          },
          pdfToImages: { format: "png", colorMode: "color", dpi: 150 },
          crop: {
            format: "webp",
            colorMode: "color",
            dpi: 150,
            photoNative: true,
            photoOnly: false,
          },
        },
      },
    ]);
  });

  it("does not save a change of the folder or of the panel being open", async () => {
    const calls = mockCropIpc({
      pick_output_dir: () => ({ dirLabel: "figures" }),
    });
    await renderApp(null);

    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: /書き出しの設定/ }));
    await flush();

    expect(callsOf(calls, "save_settings")).toHaveLength(0);
  });
});

describe("a resolution of your own", () => {
  function chooseCustom() {
    fireEvent.change(screen.getByRole("combobox", { name: "解像度" }), {
      target: { value: "custom" },
    });
    return screen.getByRole("textbox", { name: "解像度（dpi）" });
  }

  it("starts from the resolution in use and follows a valid entry", async () => {
    const calls = mockCropIpc();
    await renderApp(null);

    const entry = chooseCustom();
    expect(entry).toHaveValue("150");
    fireEvent.change(entry, { target: { value: "220" } });
    await flush();

    expect(savedCrops(calls)).toEqual([expect.objectContaining({ dpi: 220 })]);
    expect(screen.getByRole("combobox", { name: "解像度" })).toHaveValue(
      "custom",
    );
  });

  it("flags an entry out of range and keeps the last good value", async () => {
    const calls = mockCropIpc();
    await renderApp(null);
    const entry = chooseCustom();

    fireEvent.change(entry, { target: { value: "5000" } });
    fireEvent.blur(entry);
    await flush();

    expect(screen.getByRole("alert")).toHaveTextContent(
      "36〜1000 の整数を入れてください",
    );
    expect(callsOf(calls, "save_settings")).toHaveLength(0);

    fireEvent.change(entry, { target: { value: "０３００" } });
    await flush();
    expect(screen.queryByText(/整数を入れてください/)).toBeNull();
    expect(savedCrops(calls)).toEqual([expect.objectContaining({ dpi: 300 })]);
  });

  it("starts a saved resolution that is not a preset as a custom entry", async () => {
    mockCropIpc();
    await renderApp(withCrop({ dpi: 220 }));

    expect(screen.getByRole("combobox", { name: "解像度" })).toHaveValue(
      "custom",
    );
    expect(screen.getByRole("textbox", { name: "解像度（dpi）" })).toHaveValue(
      "220",
    );
  });
});

describe("during a conversion", () => {
  it("leaves the settings as they are, since a conversion locks the screens", async () => {
    mockCropIpc();
    const state = createInitialAppState("ja");
    render(
      <AppStateProvider
        initialState={{
          ...state,
          crop: { ...state.crop, settingsOpen: true },
          job: { ...initialJobState, phase: "running", kind: "pdfToImages" },
        }}
      >
        <CropExportSettings />
      </AppStateProvider>,
    );

    expect(screen.getByRole("button", { name: "JPEG" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "解像度" })).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "写真だけを書き出す" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "フォルダを選ぶ" }),
    ).toBeDisabled();
  });

  it("shows the running conversion in the bottom bar as the other tabs do", async () => {
    mockCropIpc();
    const state = createInitialAppState("ja");
    render(
      <AppStateProvider
        initialState={{
          ...state,
          job: { ...initialJobState, phase: "running", kind: "pdfToImages" },
        }}
      >
        <JobFooter
          tab="crop"
          idleStatus={<CropStatus />}
          action={<CropAction />}
        />
      </AppStateProvider>,
    );

    expect(screen.getByText(ja.job.running)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "キャンセル" })).toBeEnabled();
    expect(screen.queryByText("PDF が開かれていません")).toBeNull();
    expect(screen.queryByRole("button", { name: "書き出す" })).toBeNull();
  });
});
