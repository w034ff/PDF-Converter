import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  addImages,
  addPdfs,
  cancelJob,
  checkPageRange,
  getSettings,
  getThumbnail,
  isErrorCode,
  isIpcError,
  ITEMS_DROPPED_EVENT,
  JOB_FINISHED_EVENT,
  JOB_ITEM_EVENT,
  JOB_PROGRESS_EVENT,
  normalizeIpcError,
  onItemsDropped,
  onJobFinished,
  onJobItem,
  onJobProgress,
  pickOutputDir,
  removeItems,
  saveMergedPdf,
  saveSettings,
  startImagesToPdfs,
  startPdfsToImages,
  type AddResult,
  type ImageItem,
  type ItemsDropped,
  type JobFinishedPayload,
  type JobItemPayload,
  type JobProgressPayload,
  type PdfItem,
  type Settings,
  type SettingsInput,
} from ".";

const NO_SKIPS = { unsupported: 0, folders: 0, duplicates: 0 };

describe("ipc", () => {
  afterEach(() => clearMocks());

  describe("commands", () => {
    it("asks add_images for the source and returns what Rust added", async () => {
      const result: AddResult<ImageItem> = {
        added: [
          {
            id: 1,
            name: "a.png",
            width: 2,
            height: 3,
            format: "png",
            bytes: 10,
            error: null,
          },
        ],
        skipped: NO_SKIPS,
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return result;
      });

      expect(await addImages("folder")).toEqual(result);
      expect(calls).toEqual([["add_images", { source: "folder" }]]);
    });

    it("asks add_pdfs for the source and returns null when cancelled", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return null;
      });

      const result: AddResult<PdfItem> | null = await addPdfs("files");
      expect(result).toBeNull();
      expect(calls).toEqual([["add_pdfs", { source: "files" }]]);
    });

    it("sends only IDs to remove_items", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await removeItems([3, 4]);
      expect(calls).toEqual([["remove_items", { ids: [3, 4] }]]);
    });

    it("asks get_thumbnail for the item and the page", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return new ArrayBuffer(4);
      });

      const bytes = await getThumbnail(5, 2);
      expect(bytes.byteLength).toBe(4);
      await getThumbnail(5);
      expect(calls).toEqual([
        ["get_thumbnail", { id: 5, page: 2 }],
        ["get_thumbnail", { id: 5, page: undefined }],
      ]);
    });

    it("rejects with the { code, detail } Rust returned", async () => {
      mockIPC(() => {
        throw { code: "UnknownHandle", detail: null };
      });

      await expect(getThumbnail(9)).rejects.toEqual({
        code: "UnknownHandle",
        detail: null,
      });
    });

    it("rejects with InvalidParams when Tauri cannot read the arguments", async () => {
      mockIPC(() => {
        throw "invalid args `source` for command `add_images`";
      });

      await expect(addImages("files")).rejects.toEqual({
        code: "InvalidParams",
        detail: "invalid args `source` for command `add_images`",
      });
    });

    it("calls checkPageRange and returns result", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return { totalPages: 5 };
      });

      const res = await checkPageRange("1-3, 5", [1, 2]);
      expect(res).toEqual({ totalPages: 5 });
      expect(calls).toEqual([
        ["check_page_range", { text: "1-3, 5", ids: [1, 2] }],
      ]);
    });

    it("asks get_settings for the settings, which hold labels and no path", async () => {
      const settings: Settings = {
        language: null,
        imagesToPdf: {
          output: "merge",
          pageSize: "fit",
          outputDir: { dirLabel: "out" },
        },
        pdfToImages: { format: "png", dpi: 150, outputDir: null },
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return settings;
      });

      expect(await getSettings()).toEqual(settings);
      expect(calls).toEqual([["get_settings", {}]]);
    });

    it("sends the settings to save_settings under one argument, without folders", async () => {
      const settings: SettingsInput = {
        language: "ja",
        imagesToPdf: { output: "each", pageSize: "a4" },
        pdfToImages: { format: "jpeg", dpi: 300 },
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await saveSettings(settings);
      expect(calls).toEqual([["save_settings", { settings }]]);
    });

    it("rejects save_settings with the InvalidParams Rust returned", async () => {
      mockIPC(() => {
        throw { code: "InvalidParams", detail: null };
      });

      await expect(
        saveSettings({
          language: null,
          imagesToPdf: { output: "merge", pageSize: "fit" },
          pdfToImages: { format: "png", dpi: 100 },
        }),
      ).rejects.toEqual({ code: "InvalidParams", detail: null });
    });

    it("asks pick_output_dir for the kind and returns the label", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return { dirLabel: "out" };
      });

      expect(await pickOutputDir("pdfToImages")).toEqual({ dirLabel: "out" });
      expect(calls).toEqual([["pick_output_dir", { kind: "pdfToImages" }]]);
    });

    it("returns null from pick_output_dir when the dialog was cancelled", async () => {
      mockIPC(() => null);

      expect(await pickOutputDir("imagesToPdf")).toBeNull();
    });

    it("calls cancelJob", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await cancelJob();
      expect(calls).toEqual([["cancel_job", {}]]);
    });

    it("calls saveMergedPdf", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return { savedName: "out.pdf" };
      });

      const res = await saveMergedPdf([1, 2], "fit");
      expect(res).toEqual({ savedName: "out.pdf" });
      expect(calls).toEqual([
        ["save_merged_pdf", { ids: [1, 2], pageSize: "fit" }],
      ]);
    });

    it("calls startImagesToPdfs", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startImagesToPdfs([1, 2], "a4");
      expect(calls).toEqual([
        ["start_images_to_pdfs", { ids: [1, 2], pageSize: "a4" }],
      ]);
    });

    it("calls startPdfsToImages", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startPdfsToImages([1, 2], "1-3", "png", 150);
      expect(calls).toEqual([
        [
          "start_pdfs_to_images",
          { ids: [1, 2], range: "1-3", format: "png", dpi: 150 },
        ],
      ]);
    });
  });

  describe("errors", () => {
    it("knows the codes of design §6.6", () => {
      expect(isErrorCode("PasswordProtected")).toBe(true);
      expect(isErrorCode("ConversionRunning")).toBe(true);
      expect(isErrorCode("PdfiumUnavailable")).toBe(false);
      expect(isErrorCode("toString")).toBe(false);
      expect(isErrorCode(1)).toBe(false);
    });

    it("recognizes the shape of an error", () => {
      expect(isIpcError({ code: "TooLarge", detail: "80,000,000" })).toBe(true);
      expect(isIpcError({ code: "TooLarge" })).toBe(false);
      expect(isIpcError({ code: "Nope", detail: null })).toBe(false);
      expect(isIpcError({ code: "TooLarge", detail: 1 })).toBe(false);
      expect(isIpcError(null)).toBe(false);
    });

    it("turns anything else into InvalidParams", () => {
      expect(normalizeIpcError("text")).toEqual({
        code: "InvalidParams",
        detail: "text",
      });
      expect(normalizeIpcError(new Error("boom"))).toEqual({
        code: "InvalidParams",
        detail: "boom",
      });
      expect(normalizeIpcError({ other: 1 })).toEqual({
        code: "InvalidParams",
        detail: '{"other":1}',
      });
      expect(normalizeIpcError(undefined)).toEqual({
        code: "InvalidParams",
        detail: "undefined",
      });
    });
  });

  describe("events", () => {
    it("passes the payload of items-dropped to the handler", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: ItemsDropped[] = [];
      const unlisten = await onItemsDropped((payload) => {
        received.push(payload);
      });
      const payload: ItemsDropped = {
        images: [],
        pdfs: [],
        skipped: { unsupported: 1, folders: 0, duplicates: 0 },
        error: null,
      };

      await emit(ITEMS_DROPPED_EVENT, payload);
      unlisten();
      await emit(ITEMS_DROPPED_EVENT, payload);

      expect(received).toEqual([payload]);
    });

    it("handles job-progress events", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: JobProgressPayload[] = [];
      const unlisten = await onJobProgress((payload) => {
        received.push(payload);
      });
      const payload: JobProgressPayload = {
        done: 1,
        total: 5,
        current: "a.pdf",
      };

      await emit(JOB_PROGRESS_EVENT, payload);
      unlisten();

      expect(received).toEqual([payload]);
    });

    it("handles job-item events without optional error and failedPages", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: JobItemPayload[] = [];
      const unlisten = await onJobItem((payload) => {
        received.push(payload);
      });
      const payload: JobItemPayload = {
        id: 1,
        status: "ok",
        outputs: ["a_p1.png"],
      };

      await emit(JOB_ITEM_EVENT, payload);
      unlisten();

      expect(received).toEqual([payload]);
      expect(received[0].error).toBeUndefined();
      expect(received[0].failedPages).toBeUndefined();
    });

    it("handles job-item events with error and failedPages", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: JobItemPayload[] = [];
      const unlisten = await onJobItem((payload) => {
        received.push(payload);
      });
      const payload: JobItemPayload = {
        id: 2,
        status: "partial",
        outputs: ["a_p1.png"],
        error: { code: "RenderTooLarge", detail: "too large" },
        failedPages: [2],
      };

      await emit(JOB_ITEM_EVENT, payload);
      unlisten();

      expect(received).toEqual([payload]);
      expect(received[0].error).toEqual({
        code: "RenderTooLarge",
        detail: "too large",
      });
      expect(received[0].failedPages).toEqual([2]);
    });

    it("handles job-finished events", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: JobFinishedPayload[] = [];
      const unlisten = await onJobFinished((payload) => {
        received.push(payload);
      });
      const payload: JobFinishedPayload = {
        succeeded: 2,
        failed: 1,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      };

      await emit(JOB_FINISHED_EVENT, payload);
      unlisten();

      expect(received).toEqual([payload]);
    });
  });
});
