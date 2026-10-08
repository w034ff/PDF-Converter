import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  JOB_FINISHED_EVENT,
  JOB_ITEM_EVENT,
  JOB_PROGRESS_EVENT,
  type AddResult,
  type ImageItem,
  type JobFinishedPayload,
  type JobProgressPayload,
} from "../../ipc";
import { AppStateProvider, createInitialAppState } from "../../state";
import { JobFooter } from "../job/JobFooter";
import { useJobEvents } from "../job/useJobEvents";
import { ImagesToPdfAction } from "./ImagesToPdfAction";
import { ImagesToPdfSettings } from "./ImagesToPdfSettings";
import { ImagesToPdfStatus } from "./ImagesToPdfStatus";
import { ImagesToPdfView } from "./ImagesToPdfView";

const sampleImage1: ImageItem = {
  id: 1,
  name: "photo.jpg",
  width: 2480,
  height: 3508,
  format: "jpeg",
  bytes: 1258291, // 1.2 MB
  error: null,
};

const sampleImage2: ImageItem = {
  id: 2,
  name: "diagram.png",
  width: 1600,
  height: 900,
  format: "png",
  bytes: 245760, // 240 KB
  error: null,
};

const sampleImage3: ImageItem = {
  id: 3,
  name: "corrupt.png",
  width: 0,
  height: 0,
  format: null,
  bytes: 1024,
  error: { code: "DecodeFailed", detail: null },
};

interface IpcCall {
  cmd: string;
  args: unknown;
}

function mockAppIpc(
  answer: (cmd: string, args: unknown) => unknown,
): IpcCall[] {
  const calls: IpcCall[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      return answer(cmd, args);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

function Harness() {
  useJobEvents();
  return (
    <div>
      <aside>
        <ImagesToPdfSettings />
      </aside>
      <main>
        <ImagesToPdfView />
      </main>
      <footer>
        <JobFooter
          tab="imagesToPdf"
          idleStatus={<ImagesToPdfStatus />}
          action={<ImagesToPdfAction />}
        />
      </footer>
    </div>
  );
}

function renderHarness(initialLang = "ja-JP") {
  return render(
    <AppStateProvider initialState={createInitialAppState(initialLang)}>
      <Harness />
    </AppStateProvider>,
  );
}

describe("ImagesToPdf (T11)", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("leaves a loading thumbnail blank and marks only one that failed", async () => {
    mockAppIpc((cmd, args) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage3],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
        return result;
      }
      if (cmd === "get_thumbnail") {
        const isLoading =
          typeof args === "object" &&
          args !== null &&
          "id" in args &&
          args.id === sampleImage1.id;
        // photo.jpg never arrives; corrupt.png cannot be made.
        return isLoading
          ? new Promise(() => {})
          : Promise.reject({ code: "DecodeFailed", detail: null });
      }
      return undefined;
    });
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));

    const failed = await screen.findByRole("img", {
      name: "corrupt.png のサムネイル",
    });
    await waitFor(() => {
      expect(failed.querySelector("svg")).not.toBeNull();
    });
    const loading = screen.getByRole("img", { name: "photo.jpg のサムネイル" });
    expect(loading.querySelector("svg")).toBeNull();
  });

  describe("errors and results", () => {
    function mockRun(fail: Record<string, unknown> = {}) {
      return mockAppIpc((cmd) => {
        if (cmd in fail) {
          return Promise.reject(fail[cmd]);
        }
        if (cmd === "add_images") {
          const result: AddResult<ImageItem> = {
            added: [sampleImage1, sampleImage2, sampleImage3],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return result;
        }
        if (cmd === "pick_output_dir") {
          return { dirLabel: "out" };
        }
        if (cmd === "save_merged_pdf") {
          return new Promise(() => {});
        }
        return undefined;
      });
    }

    async function addImages() {
      fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
      await screen.findByText("photo.jpg");
    }

    it("shows a failed add on the empty list, and clears it once an add works", async () => {
      mockRun({ add_images: { code: "ReadFailed", detail: null } });
      renderHarness();

      fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
      expect(await screen.findByTestId("error-display")).toBeInTheDocument();
      expect(screen.getByTestId("drop-zone")).toBeInTheDocument();

      cleanup();
      clearMocks();
      mockRun();
      renderHarness();
      await addImages();
      expect(screen.queryByTestId("error-display")).not.toBeInTheDocument();
    });

    it("gives the reason for a failed conversion and for an unreadable image in the table", async () => {
      mockRun();
      renderHarness();
      await addImages();
      fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
      fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "キャンセル" })),
      );

      await act(async () => {
        await emit(JOB_ITEM_EVENT, {
          id: 1,
          status: "ok",
          outputs: ["photo.pdf"],
        });
        await emit(JOB_ITEM_EVENT, {
          id: 2,
          status: "failed",
          outputs: [],
          error: { code: "WriteFailed", detail: null },
        });
        await emit(JOB_FINISHED_EVENT, {
          succeeded: 1,
          failed: 1,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        });
      });

      expect(screen.getByText("✕ 失敗")).toBeInTheDocument();
      expect(screen.getByText("ファイルの書き込みに失敗しました")).toHaveClass(
        "images-table-reason",
      );
      expect(screen.getByText("✕ 読み込めません")).toBeInTheDocument();
      expect(screen.getByRole("status")).toHaveClass("is-failure");
    });

    it("does not mark merged pages as done when the merge was cancelled", async () => {
      mockRun();
      renderHarness();
      await addImages();
      fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "キャンセル" })),
      );

      await act(async () => {
        await emit(JOB_ITEM_EVENT, { id: 1, status: "ok", outputs: [] });
        await emit(JOB_ITEM_EVENT, { id: 2, status: "ok", outputs: [] });
        await emit(JOB_FINISHED_EVENT, {
          succeeded: 0,
          failed: 0,
          noPages: 0,
          unprocessed: 2,
          cancelled: true,
        });
      });

      expect(screen.queryByText("✓ 完了")).not.toBeInTheDocument();
      expect(screen.getAllByText("キャンセル")).toHaveLength(2);
    });

    it("drops the last result once the list changes", async () => {
      mockRun();
      renderHarness();
      await addImages();
      fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
      fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "キャンセル" })),
      );
      await act(async () => {
        await emit(JOB_ITEM_EVENT, { id: 1, status: "ok", outputs: ["a.pdf"] });
        await emit(JOB_FINISHED_EVENT, {
          succeeded: 1,
          failed: 0,
          noPages: 0,
          unprocessed: 1,
          cancelled: false,
        });
      });
      expect(screen.getByRole("status")).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "1 つの PDF" }));

      expect(screen.queryByRole("status")).not.toBeInTheDocument();
      expect(screen.getByText("2 ページの PDF になります")).toBeInTheDocument();
    });
  });

  it("shows DropZone in empty state, and adds images via dialog", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage2],
          skipped: {
            unsupported: 1,
            folders: 1,
            duplicates: 0,
          },
        };
        return result;
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    // DropZone is rendered
    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
    expect(screen.getByText("画像をここにドロップ")).toBeInTheDocument();

    // Click "画像を追加" in DropZone
    const addBtn = screen.getByRole("button", { name: "画像を追加" });
    fireEvent.click(addBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "add_images",
        args: { source: "files" },
      });
    });

    // Items are now displayed
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
      expect(screen.getByText("diagram.png")).toBeInTheDocument();
    });

    // Skipped message is displayed in header
    expect(
      screen.getByText("（対象外 2 件：サブフォルダ、非対応の形式）"),
    ).toBeInTheDocument();

    // Footer shows page count, and summary is no longer below the list
    expect(screen.getByText("2 ページの PDF になります")).toBeInTheDocument();
    expect(
      screen.getByText("2 ページの PDF になります").closest("footer"),
    ).not.toBeNull();
    expect(document.querySelector(".images-merge-summary")).toBeNull();
  });

  it("removes individual items and clears all items with remove_items IPC", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
        return result;
      }
      if (cmd === "remove_items") {
        return undefined;
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    // Add 2 images
    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Remove first item
    const removeButtons = screen.getAllByRole("button", {
      name: "一覧から外す",
    });
    fireEvent.click(removeButtons[0]);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [1] },
      });
    });

    await waitFor(() => {
      expect(screen.queryByText("photo.jpg")).not.toBeInTheDocument();
      expect(screen.getByText("diagram.png")).toBeInTheDocument();
    });

    // Clear all items
    const clearAllBtn = screen.getByRole("button", { name: "すべて外す" });
    fireEvent.click(clearAllBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [2] },
      });
    });

    // Returns to DropZone
    await waitFor(() => {
      expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
    });
  });

  it("reorders via Up/Down buttons, Alt + Up/Down, and pointer drag, sending ordered IDs to save_merged_pdf", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
        return result;
      }
      if (cmd === "save_merged_pdf") {
        return { savedName: "output.pdf" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    // Add images
    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // 1. Reorder using Down button on first item
    const downButtons = screen.getAllByRole("button", { name: "下へ" });
    fireEvent.click(downButtons[0]);

    // photo.jpg is now second, diagram.png is first
    let rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("diagram.png");
    expect(rows[1]).toHaveTextContent("photo.jpg");

    // 2. Reorder using Alt + Up keyboard navigation
    rows[1].focus();
    fireEvent.keyDown(rows[1], { key: "ArrowUp", altKey: true });

    // photo.jpg is now first again
    rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("photo.jpg");
    expect(rows[1]).toHaveTextContent("diagram.png");

    // 3. Reorder using pointer drag & drop
    // pointerDown on first row, pointerUp on second row
    fireEvent.pointerDown(rows[0], { button: 0 });
    fireEvent.pointerMove(rows[1]);
    fireEvent.pointerUp(rows[1]);

    // diagram.png is now first, photo.jpg is second
    rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("diagram.png");
    expect(rows[1]).toHaveTextContent("photo.jpg");

    // Click "PDF を保存"
    const saveBtn = screen.getByRole("button", { name: "PDF を保存" });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "save_merged_pdf",
        args: { ids: [2, 1], pageSize: "fit" },
      });
    });
  });

  it("excludes error rows from conversion targets", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage2, sampleImage3],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
        return result;
      }
      if (cmd === "save_merged_pdf") {
        return { savedName: "merged.pdf" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("corrupt.png")).toBeInTheDocument();
    });

    // Error reason is shown for corrupt.png
    expect(
      screen.getByText(
        "画像を読み込めませんでした。ファイルが壊れている可能性があります",
      ),
    ).toBeInTheDocument();

    // Footer shows 2 pages (excluding error row)
    expect(screen.getByText("2 ページの PDF になります")).toBeInTheDocument();
    expect(
      screen.getByText("2 ページの PDF になります").closest("footer"),
    ).not.toBeNull();
    expect(document.querySelector(".images-merge-summary")).toBeNull();

    // Click save
    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));

    await waitFor(() => {
      // Targets only contain valid items 1 and 2, corrupt item 3 is excluded
      expect(calls).toContainEqual({
        cmd: "save_merged_pdf",
        args: { ids: [1, 2], pageSize: "fit" },
      });
    });
  });

  it("opens folder dialog when starting without output directory in 'each' mode, updates settings panel, and starts conversion", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
        return result;
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "export-folder" };
      }
      if (cmd === "start_images_to_pdfs") {
        return undefined;
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    // Add an image
    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Switch output mode to "1 枚ずつ"
    const eachModeBtn = screen.getByRole("button", { name: "1 枚ずつ" });
    fireEvent.click(eachModeBtn);

    // Button label is now "変換を開始", and is ENABLED even without output dir
    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    expect(startBtn).toBeEnabled();

    // Click start conversion without previously selecting folder
    fireEvent.click(startBtn);

    // pick_output_dir is called with kind: "imagesToPdf"
    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "pick_output_dir",
        args: { kind: "imagesToPdf" },
      });
    });

    // Settings panel output dir name is updated
    await waitFor(() => {
      expect(screen.getByText("export-folder")).toBeInTheDocument();
    });

    // start_images_to_pdfs is called with valid target ids and page size
    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_images_to_pdfs",
        args: { ids: [1], pageSize: "fit" },
      });
    });
  });

  it("does not start conversion when pick_output_dir is cancelled in 'each' mode", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        return null;
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "pick_output_dir",
        args: { kind: "imagesToPdf" },
      });
    });

    expect(calls.some((c) => c.cmd === "start_images_to_pdfs")).toBe(false);
  });

  it("shows error in bottom bar when pick_output_dir fails in 'each' mode", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        throw { code: "ReadFailed", detail: "read failed" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "pick_output_dir",
        args: { kind: "imagesToPdf" },
      });
    });

    expect(calls.some((c) => c.cmd === "start_images_to_pdfs")).toBe(false);

    // Error is displayed in bottom bar
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        "ファイルの読み込みに失敗しました",
      );
    });
  });

  it("disables conversion button while waiting for pick_output_dir dialog in 'each' mode", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        return new Promise(() => {}); // never resolves
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    expect(startBtn).toBeEnabled();

    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(startBtn).toBeDisabled();
    });
  });

  it("starts conversion directly without pick_output_dir when directory is already chosen in 'each' mode", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "already-chosen" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    // Manually pick folder first
    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
    await waitFor(() => {
      expect(screen.getByText("already-chosen")).toBeInTheDocument();
    });

    calls.length = 0; // reset call log

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_images_to_pdfs",
        args: { ids: [1], pageSize: "fit" },
      });
    });

    expect(calls.some((c) => c.cmd === "pick_output_dir")).toBe(false);
  });

  describe("a start refused because of the output folder", () => {
    const MISSING_MESSAGE =
      "保存先のフォルダが見つかりません。フォルダを選び直してください";
    const NOT_WRITABLE_MESSAGE = "保存先のフォルダに書き込めません";

    /**
     * Answers the commands of an "each" conversion. `startAnswers` are the
     * answers of `start_images_to_pdfs` in turn, `pickAnswers` those of
     * `pick_output_dir`; the last one repeats.
     */
    function mockEach(
      startAnswers: (() => unknown)[],
      pickAnswers: (() => unknown)[],
    ): IpcCall[] {
      let starts = 0;
      let picks = 0;
      return mockAppIpc((cmd) => {
        if (cmd === "add_images") {
          const result: AddResult<ImageItem> = {
            added: [sampleImage1],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return result;
        }
        if (cmd === "get_thumbnail") {
          return [137, 80, 78, 71];
        }
        if (cmd === "start_images_to_pdfs") {
          const answer =
            startAnswers[Math.min(starts, startAnswers.length - 1)];
          starts += 1;
          return answer();
        }
        if (cmd === "pick_output_dir") {
          const answer = pickAnswers[Math.min(picks, pickAnswers.length - 1)];
          picks += 1;
          return answer();
        }
        return undefined;
      });
    }

    async function chooseFolderAndStart() {
      renderHarness();
      fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
      await screen.findByText("photo.jpg");
      fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
      fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
      await screen.findByText("old-folder");
      fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));
    }

    const countOf = (calls: IpcCall[], cmd: string) =>
      calls.filter((call) => call.cmd === cmd).length;

    it("asks for a folder again when it is gone, then starts with the new one", async () => {
      let answerPick: (label: unknown) => void = () => {};
      const calls = mockEach(
        [
          () => Promise.reject({ code: "OutputDirMissing", detail: null }),
          () => undefined,
        ],
        [
          () => ({ dirLabel: "old-folder" }),
          () => new Promise((resolve) => (answerPick = resolve)),
        ],
      );

      await chooseFolderAndStart();

      // The dialog is open: the field is empty and the reason is on screen.
      await waitFor(() => {
        expect(countOf(calls, "pick_output_dir")).toBe(2);
      });
      expect(screen.getByTestId("output-dir-name")).toHaveTextContent("未選択");
      expect(screen.getAllByText(MISSING_MESSAGE)).toHaveLength(1);
      expect(screen.getByTestId("error-display")).toHaveTextContent(
        MISSING_MESSAGE,
      );
      expect(countOf(calls, "start_images_to_pdfs")).toBe(1);

      await act(async () => {
        answerPick({ dirLabel: "new-folder" });
      });

      await waitFor(() => {
        expect(countOf(calls, "start_images_to_pdfs")).toBe(2);
      });
      expect(screen.getByTestId("output-dir-name")).toHaveTextContent(
        "new-folder",
      );
      expect(screen.queryByText(MISSING_MESSAGE)).toBeNull();
    });

    it("starts nothing when the folder dialog is cancelled", async () => {
      const calls = mockEach(
        [() => Promise.reject({ code: "OutputDirMissing", detail: null })],
        [() => ({ dirLabel: "old-folder" }), () => null],
      );

      await chooseFolderAndStart();

      await waitFor(() => {
        expect(countOf(calls, "pick_output_dir")).toBe(2);
      });
      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: "変換を開始" }),
        ).toBeEnabled();
      });
      expect(countOf(calls, "start_images_to_pdfs")).toBe(1);
      expect(screen.getByTestId("output-dir-name")).toHaveTextContent("未選択");
      expect(screen.getAllByText(MISSING_MESSAGE)).toHaveLength(1);
    });

    it("does not ask again for a folder it has just been given", async () => {
      const calls = mockEach(
        [() => Promise.reject({ code: "OutputDirMissing", detail: null })],
        [
          () => ({ dirLabel: "old-folder" }),
          () => ({ dirLabel: "new-folder" }),
        ],
      );

      await chooseFolderAndStart();

      await waitFor(() => {
        expect(countOf(calls, "start_images_to_pdfs")).toBe(2);
      });
      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: "変換を開始" }),
        ).toBeEnabled();
      });
      expect(countOf(calls, "pick_output_dir")).toBe(2);
      expect(screen.getByTestId("output-dir-name")).toHaveTextContent("未選択");
      expect(screen.getAllByText(MISSING_MESSAGE)).toHaveLength(1);
    });

    it("shows that the folder cannot be written to and starts nothing else", async () => {
      const calls = mockEach(
        [() => Promise.reject({ code: "OutputDirNotWritable", detail: null })],
        [() => ({ dirLabel: "old-folder" })],
      );

      await chooseFolderAndStart();

      expect(await screen.findByText(NOT_WRITABLE_MESSAGE)).toBeInTheDocument();
      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: "変換を開始" }),
        ).toBeEnabled();
      });
      expect(screen.getAllByText(NOT_WRITABLE_MESSAGE)).toHaveLength(1);
      expect(screen.getByTestId("error-display")).toHaveTextContent(
        NOT_WRITABLE_MESSAGE,
      );
      expect(countOf(calls, "start_images_to_pdfs")).toBe(1);
      expect(countOf(calls, "pick_output_dir")).toBe(1);
      expect(screen.getByTestId("output-dir-name")).toHaveTextContent(
        "old-folder",
      );
    });
  });

  it("applies is-dragging, is-drop-after, and is-drop-before during drag, and cleans up after drop", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1, sampleImage2, sampleImage3],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    const rows = screen.getAllByRole("listitem");
    // Row 0: photo.jpg, Row 1: diagram.png, Row 2: corrupt.png

    // Drag row 0 down over row 1
    fireEvent.pointerDown(rows[0], { button: 0 });
    expect(rows[0]).toHaveClass("is-dragging");

    fireEvent.pointerMove(rows[1]);
    expect(rows[0]).toHaveClass("is-dragging");
    expect(rows[1]).toHaveClass("is-drop-after");
    expect(rows[1]).not.toHaveClass("is-drop-before");

    // Drag row 0 down over row 2
    fireEvent.pointerMove(rows[2]);
    expect(rows[1]).not.toHaveClass("is-drop-after");
    expect(rows[2]).toHaveClass("is-drop-after");

    // Drag back over row 0 (original position: no drop indicator)
    fireEvent.pointerMove(rows[0]);
    expect(rows[0]).toHaveClass("is-dragging");
    expect(rows[0]).not.toHaveClass("is-drop-before");
    expect(rows[0]).not.toHaveClass("is-drop-after");

    // Release
    fireEvent.pointerUp(rows[0]);
    expect(rows[0]).not.toHaveClass("is-dragging");
    expect(rows[1]).not.toHaveClass("is-drop-after");
    expect(rows[2]).not.toHaveClass("is-drop-after");

    // Now drag row 2 up over row 1
    fireEvent.pointerDown(rows[2], { button: 0 });
    expect(rows[2]).toHaveClass("is-dragging");

    fireEvent.pointerMove(rows[1]);
    expect(rows[1]).toHaveClass("is-drop-before");
    expect(rows[1]).not.toHaveClass("is-drop-after");

    fireEvent.pointerUp(rows[1]);
    expect(rows[1]).not.toHaveClass("is-drop-before");
    expect(rows[2]).not.toHaveClass("is-dragging");
  });

  it("cancels drag on pointercancel without reordering items and clears drop indicators", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    let rows = screen.getAllByRole("listitem");
    // Row 0: photo.jpg, Row 1: diagram.png

    // Drag row 0 over row 1
    fireEvent.pointerDown(rows[0], { button: 0 });
    expect(rows[0]).toHaveClass("is-dragging");

    fireEvent.pointerMove(rows[1]);
    expect(rows[1]).toHaveClass("is-drop-after");

    // Send pointercancel
    const list = screen.getByRole("list");
    fireEvent.pointerCancel(list);

    // Order remains unchanged: photo.jpg is still first, diagram.png is second
    rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("photo.jpg");
    expect(rows[1]).toHaveTextContent("diagram.png");

    // Indicators are completely cleared
    expect(rows[0]).not.toHaveClass("is-dragging");
    expect(rows[1]).not.toHaveClass("is-drop-after");
    expect(rows[1]).not.toHaveClass("is-drop-before");
  });

  it("keeps focus on moved row after Alt + Up and Alt + Down keyboard reordering", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    let rows = screen.getAllByRole("listitem");
    // Row 0 is photo.jpg, Row 1 is diagram.png
    rows[0].focus();
    expect(rows[0]).toHaveFocus();

    // Blur first so that only the focus restoration logic moves focus
    rows[0].blur();
    expect(rows[0]).not.toHaveFocus();

    // Alt + Down moves photo.jpg to row 1
    fireEvent.keyDown(rows[0], { key: "ArrowDown", altKey: true });

    rows = screen.getAllByRole("listitem");
    expect(rows[1]).toHaveTextContent("photo.jpg");
    expect(rows[1]).toHaveFocus();

    // Blur first so that only the focus restoration logic moves focus
    rows[1].blur();
    expect(rows[1]).not.toHaveFocus();

    // Alt + Up moves photo.jpg back to row 0
    fireEvent.keyDown(rows[1], { key: "ArrowUp", altKey: true });

    rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("photo.jpg");
    expect(rows[0]).toHaveFocus();
  });

  it("renders thumbnails and remove buttons in 'each' mode table, and handles removal and conversion disable", async () => {
    let resolveConversion: () => void = () => {};
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1, sampleImage3], // photo.jpg (ok) and corrupt.png (error)
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      if (cmd === "remove_items") {
        return undefined;
      }
      if (cmd === "start_images_to_pdfs") {
        return new Promise<void>((resolve) => {
          resolveConversion = resolve;
        });
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "out" };
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Switch to each mode
    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));

    // Check table headers: "ファイル名", "保存するファイル名", "状態", "操作"
    expect(screen.getByText("操作")).toBeInTheDocument();

    // Both rows have thumbnails (img or placeholder svg)
    const photoThumb = screen.getByRole("img", {
      name: "photo.jpg のサムネイル",
    });
    expect(photoThumb).toBeInTheDocument();
    const corruptThumb = screen.getByRole("img", {
      name: "corrupt.png のサムネイル",
    });
    expect(corruptThumb).toBeInTheDocument();

    // Both rows have "外す" button enabled before conversion
    const removeButtons = screen.getAllByRole("button", { name: "外す" });
    expect(removeButtons).toHaveLength(2);
    expect(removeButtons[0]).toBeEnabled();
    expect(removeButtons[1]).toBeEnabled();

    // Click "外す" on corrupt.png (second row)
    fireEvent.click(removeButtons[1]);
    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [3] },
      });
    });
    await waitFor(() => {
      expect(screen.queryByText("corrupt.png")).not.toBeInTheDocument();
    });

    // Start conversion
    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    // During conversion: "外す" is disabled
    await waitFor(() => {
      const remainingRemoveBtn = screen.getByRole("button", { name: "外す" });
      expect(remainingRemoveBtn).toBeDisabled();
    });

    // Complete job
    await act(async () => {
      resolveConversion();
      await emit(JOB_FINISHED_EVENT, {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      } satisfies JobFinishedPayload);
    });

    // After conversion: "外す" is enabled again
    const postJobRemoveBtn = screen.getByRole("button", { name: "外す" });
    expect(postJobRemoveBtn).toBeEnabled();
  });

  it("retains row and displays error when remove in table fails", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      if (cmd === "remove_items") {
        throw { code: "Failed", detail: "failed to remove item" };
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Switch to each mode
    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));

    const removeBtn = screen.getByRole("button", { name: "外す" });
    fireEvent.click(removeBtn);

    // Item remains in table and error alert is displayed
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("disables list actions and settings during conversion", async () => {
    let resolveConversion: () => void = () => {};
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "out" };
      }
      if (cmd === "start_images_to_pdfs") {
        return new Promise<void>((resolve) => {
          resolveConversion = resolve;
        });
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Switch to each mode and pick dir
    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
    await waitFor(() => {
      expect(screen.getByText("out")).toBeInTheDocument();
    });

    // Start conversion
    fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));

    // Now conversion is active!
    // 1. Settings panel disabled note is displayed
    expect(
      screen.getByText("変換中は設定を変えられません"),
    ).toBeInTheDocument();

    // 2. Settings controls are disabled
    expect(screen.getByRole("button", { name: "1 つの PDF" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "1 枚ずつ" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "画像に合わせる" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "A4" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "フォルダを選ぶ" }),
    ).toBeDisabled();

    // 3. List action buttons are disabled
    expect(
      screen.getAllByRole("button", { name: "画像を追加" })[0],
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "フォルダを追加" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "すべて外す" })).toBeDisabled();

    // Complete job
    await act(async () => {
      resolveConversion();
    });
  });

  it("sets tabIndex 0 before conversion and removes tabIndex during conversion", async () => {
    let resolveConversion: () => void = () => {};
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      if (cmd === "save_merged_pdf") {
        return new Promise<{ savedName: string }>((resolve) => {
          resolveConversion = () => resolve({ savedName: "merged.pdf" });
        });
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Before conversion: rows have tabindex="0"
    const rowBefore = screen.getByRole("listitem");
    expect(rowBefore).toHaveAttribute("tabindex", "0");

    // Start conversion
    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));

    // During conversion: rows do not have tabindex attribute
    await waitFor(() => {
      const rowDuring = screen.getByRole("listitem");
      expect(rowDuring).not.toHaveAttribute("tabindex");
    });

    // Complete job
    await act(async () => {
      resolveConversion();
      await emit(JOB_FINISHED_EVENT, {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      } satisfies JobFinishedPayload);
    });

    // After conversion: rows have tabindex="0" again
    await waitFor(() => {
      const rowAfter = screen.getByRole("listitem");
      expect(rowAfter).toHaveAttribute("tabindex", "0");
    });
  });

  it("switches to 'キャンセル中…' immediately when cancel button is clicked", async () => {
    let resolveCancel: () => void = () => {};
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "save_merged_pdf") {
        return new Promise(() => {});
      }
      if (cmd === "cancel_job") {
        return new Promise<void>((resolve) => {
          resolveCancel = resolve;
        });
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Start conversion
    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));

    // Progress arrives
    await act(async () => {
      await emit(JOB_PROGRESS_EVENT, {
        done: 0,
        total: 1,
        current: "photo.jpg",
      } satisfies JobProgressPayload);
    });

    // Cancel button is in footer
    const cancelBtn = screen.getByRole("button", { name: "キャンセル" });
    expect(cancelBtn).toBeInTheDocument();

    // Click cancel
    fireEvent.click(cancelBtn);

    // Immediately changes to "キャンセル中…" and is disabled
    expect(
      screen.getByRole("button", { name: "キャンセル中…" }),
    ).toBeDisabled();
    expect(calls).toContainEqual({ cmd: "cancel_job", args: {} });

    await act(async () => {
      resolveCancel();
    });
  });

  it("displays status symbols (✓ / ✕) and job completion summary with saved name", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        return {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "out" };
      }
      if (cmd === "start_images_to_pdfs") {
        return undefined;
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Switch to each mode
    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
    await waitFor(() => {
      expect(screen.getByText("out")).toBeInTheDocument();
    });

    // Table headers are displayed
    expect(screen.getByText("ファイル名")).toBeInTheDocument();
    expect(screen.getByText("保存するファイル名")).toBeInTheDocument();
    expect(screen.getByText("状態")).toBeInTheDocument();

    // Initially output filename shows "—"
    const dashes = screen.getAllByText("—");
    expect(dashes.length).toBeGreaterThan(0);

    // Start conversion
    fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));

    // Emit progress
    await act(async () => {
      await emit(JOB_PROGRESS_EVENT, {
        done: 0,
        total: 2,
        current: "photo.jpg",
      } satisfies JobProgressPayload);
    });

    // Emit item 1 ok
    await act(async () => {
      await emit(JOB_ITEM_EVENT, {
        id: 1,
        status: "ok",
        outputs: ["photo.pdf"],
      });
    });

    // Item 1 shows output filename and ✓ symbol
    expect(screen.getByText("photo.pdf")).toBeInTheDocument();
    expect(screen.getByText("✓ 完了")).toBeInTheDocument();

    // Emit item 2 failed
    await act(async () => {
      await emit(JOB_ITEM_EVENT, {
        id: 2,
        status: "failed",
        outputs: [],
      });
    });

    // Item 2 shows ✕ symbol
    expect(screen.getByText("✕ 失敗")).toBeInTheDocument();

    // Emit job finished
    await act(async () => {
      await emit(JOB_FINISHED_EVENT, {
        succeeded: 1,
        failed: 1,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      } satisfies JobFinishedPayload);
    });

    // Summary above table is displayed
    expect(
      screen.getByText("変換が終わりました：成功 1 件 · 失敗 1 件"),
    ).toBeInTheDocument();
  });

  it("retains items in list and displays error when remove_items IPC fails", async () => {
    const calls = mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage2],
          skipped: { unsupported: 1, folders: 0, duplicates: 0 },
        };
        return result;
      }
      if (cmd === "remove_items") {
        throw { code: "Failed", detail: "failed to remove item" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness();

    // Add images
    fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
      expect(screen.getByText("diagram.png")).toBeInTheDocument();
    });
    expect(
      screen.getByText("（対象外 1 件：非対応の形式）"),
    ).toBeInTheDocument();

    // 1. Try removing first item (photo.jpg)
    const removeButtons = screen.getAllByRole("button", {
      name: "一覧から外す",
    });
    fireEvent.click(removeButtons[0]);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [1] },
      });
    });

    // Item remains in the list, and error is shown
    expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    expect(screen.getByText("diagram.png")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();

    // Dismiss the error before testing clear all
    fireEvent.click(screen.getByRole("button", { name: "閉じる" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    // 2. Try clearing all items
    const clearAllBtn = screen.getByRole("button", { name: "すべて外す" });
    fireEvent.click(clearAllBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [1, 2] },
      });
    });

    // Items and skipped note remain in the list, not reverted to DropZone, and error is shown
    expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    expect(screen.getByText("diagram.png")).toBeInTheDocument();
    expect(screen.queryByTestId("drop-zone")).not.toBeInTheDocument();
    expect(
      screen.getByText("（対象外 1 件：非対応の形式）"),
    ).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  describe("summary of 'Single PDF'", () => {
    async function mergeAndFinish(
      finished: JobFinishedPayload,
      save: () => unknown,
    ) {
      mockAppIpc((cmd) => {
        if (cmd === "add_images") {
          const result: AddResult<ImageItem> = {
            added: [sampleImage1, sampleImage2],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return result;
        }
        if (cmd === "save_merged_pdf") {
          return save();
        }
        if (cmd === "get_thumbnail") {
          return [137, 80, 78, 71];
        }
        return undefined;
      });
      renderHarness();
      fireEvent.click(screen.getByRole("button", { name: "画像を追加" }));
      await screen.findByText("photo.jpg");

      fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
      await act(async () => {
        await emit(JOB_FINISHED_EVENT, finished);
      });
    }

    const NOTHING: JobFinishedPayload = {
      succeeded: 0,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    };

    it("counts the pages of the PDF and adds the images that failed", async () => {
      await mergeAndFinish({ ...NOTHING, succeeded: 10, failed: 1 }, () => ({
        savedName: "merged.pdf",
      }));

      expect(await screen.findByRole("status")).toHaveTextContent(
        "変換が終わりました：10 ページの PDF を保存しました · 失敗 1 枚",
      );
      expect(screen.queryByText(/merged\.pdf/)).not.toBeInTheDocument();
    });

    it("says only the pages when no image failed", async () => {
      await mergeAndFinish({ ...NOTHING, succeeded: 2 }, () => ({
        savedName: "merged.pdf",
      }));

      expect(await screen.findByRole("status")).toHaveTextContent(
        /^変換が終わりました：2 ページの PDF を保存しました$/,
      );
    });

    it("counts items, without a zero, when no PDF was written", async () => {
      await mergeAndFinish(
        { ...NOTHING, failed: 2 },
        () => new Promise(() => {}),
      );

      expect(await screen.findByRole("status")).toHaveTextContent(
        /^変換が終わりました：失敗 2 件$/,
      );
    });

    it("counts items when the conversion was cancelled", async () => {
      await mergeAndFinish(
        { ...NOTHING, succeeded: 0, unprocessed: 2, cancelled: true },
        () => new Promise(() => {}),
      );

      expect(await screen.findByRole("status")).toHaveTextContent(
        /^キャンセルしました：未処理 2 件$/,
      );
    });
  });

  it("does not contain full-width parentheses in the skipped notice in English locale, and does not show the saved name", async () => {
    mockAppIpc((cmd) => {
      if (cmd === "add_images") {
        const result: AddResult<ImageItem> = {
          added: [sampleImage1, sampleImage2],
          skipped: {
            unsupported: 1,
            folders: 1,
            duplicates: 0,
          },
        };
        return result;
      }
      if (cmd === "save_merged_pdf") {
        return { savedName: "merged.pdf" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    });

    renderHarness("en-US");

    // Add images in English locale
    fireEvent.click(screen.getByRole("button", { name: "Add images" }));
    await waitFor(() => {
      expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    });

    // Skipped notice has half-width parentheses and no full-width parentheses
    const skippedNotice = screen.getByText(
      "(2 skipped: subfolders, unsupported formats)",
    );
    expect(skippedNotice).toBeInTheDocument();
    expect(skippedNotice.textContent).not.toMatch(/[（）]/);

    // Save PDF
    fireEvent.click(screen.getByRole("button", { name: "Save PDF" }));

    // Finish job
    await act(async () => {
      await emit(JOB_FINISHED_EVENT, {
        succeeded: 2,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      } satisfies JobFinishedPayload);
    });

    // The summary counts the pages of the PDF and does not name the file
    expect(
      await screen.findByText("Conversion finished: Saved a 2-page PDF"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/merged\.pdf/)).not.toBeInTheDocument();
  });
});
