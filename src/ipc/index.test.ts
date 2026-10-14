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
  isPageSizePt,
  isPdfItem,
  isSettings,
  isUnlockResult,
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
  unlockPdf,
  type AddResult,
  type ImageItem,
  type ItemsDropped,
  type JobFinishedPayload,
  type JobItemPayload,
  type JobProgressPayload,
  type PageSizeChoice,
  type PdfItem,
  type Settings,
  type SettingsInput,
  type UnlockResult,
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

    it("sends id and password to unlock_pdf and returns validated UnlockResult", async () => {
      const unlockResult: UnlockResult = {
        items: [
          {
            id: 1,
            name: "encrypted.pdf",
            pageCount: 3,
            firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
            bytes: 1000,
            error: null,
          },
        ],
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return unlockResult;
      });

      const res = await unlockPdf(1, "secret password");
      expect(res).toEqual(unlockResult);
      expect(calls).toEqual([
        ["unlock_pdf", { id: 1, password: "secret password" }],
      ]);
    });

    it("rejects with InvalidParams when unlock_pdf returns malformed result", async () => {
      mockIPC(() => ({ items: "not an array" }));
      await expect(unlockPdf(1, "pass")).rejects.toEqual({
        code: "InvalidParams",
        detail: "unlock_pdf returned an unexpected value",
      });
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
        return {
          totalPages: 5,
          intervals: [
            [1, 3],
            [5, 5],
          ],
        };
      });

      const res = await checkPageRange("1-3, 5", [1, 2]);
      expect(res).toEqual({
        totalPages: 5,
        intervals: [
          [1, 3],
          [5, 5],
        ],
      });
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
          orientation: "auto",
          pdfA: false,
          outputDir: { dirLabel: "out" },
        },
        pdfToImages: {
          format: "png",
          colorMode: "color",
          dpi: 150,
          outputDir: null,
        },
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return settings;
      });

      expect(await getSettings()).toEqual(settings);
      expect(calls).toEqual([["get_settings", {}]]);
    });

    it("sends the revision and the settings, without folders, to save_settings", async () => {
      const settings: SettingsInput = {
        language: "ja",
        imagesToPdf: {
          output: "each",
          pageSize: "a4",
          orientation: "portrait",
          pdfA: true,
        },
        pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await saveSettings(3, settings);
      expect(calls).toEqual([["save_settings", { revision: 3, settings }]]);
    });

    it("rejects save_settings with the InvalidParams Rust returned", async () => {
      mockIPC(() => {
        throw { code: "InvalidParams", detail: null };
      });

      await expect(
        saveSettings(1, {
          language: null,
          imagesToPdf: {
            output: "merge",
            pageSize: "fit",
            orientation: "auto",
            pdfA: false,
          },
          pdfToImages: { format: "png", colorMode: "color", dpi: 100 },
        }),
      ).rejects.toEqual({ code: "InvalidParams", detail: null });
    });

    it("rejects save_settings with InvalidParams when Tauri cannot read arguments (e.g. missing colorMode)", async () => {
      mockIPC(() => {
        throw "invalid args `settings` for command `save_settings`: missing field `colorMode`";
      });

      await expect(
        saveSettings(1, {
          language: null,
          imagesToPdf: {
            output: "merge",
            pageSize: "fit",
            orientation: "auto",
            pdfA: false,
          },
          pdfToImages: { format: "png", colorMode: "color", dpi: 150 },
        }),
      ).rejects.toEqual({
        code: "InvalidParams",
        detail:
          "invalid args `settings` for command `save_settings`: missing field `colorMode`",
      });
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

      const res = await saveMergedPdf([1, 2], "fit", "auto", true);
      expect(res).toEqual({ savedName: "out.pdf" });
      expect(calls).toEqual([
        [
          "save_merged_pdf",
          { ids: [1, 2], pageSize: "fit", orientation: "auto", pdfA: true },
        ],
      ]);
    });

    it("tells a cancelled dialog from a merge that wrote no PDF", async () => {
      mockIPC(() => null);
      expect(await saveMergedPdf([1], "fit", "auto", false)).toBeNull();

      clearMocks();
      mockIPC(() => ({ savedName: null }));
      expect(await saveMergedPdf([1], "fit", "auto", false)).toEqual({
        savedName: null,
      });
    });

    it("calls startImagesToPdfs", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startImagesToPdfs([1, 2], "a4", "portrait", true);
      expect(calls).toEqual([
        [
          "start_images_to_pdfs",
          {
            ids: [1, 2],
            pageSize: "a4",
            orientation: "portrait",
            pdfA: true,
          },
        ],
      ]);
    });

    it("calls startPdfsToImages", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startPdfsToImages([1, 2], "1-3", "png", "color", 150);
      expect(calls).toEqual([
        [
          "start_pdfs_to_images",
          {
            ids: [1, 2],
            range: "1-3",
            format: "png",
            colorMode: "color",
            dpi: 150,
          },
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

    it("validates settings shape including orientation, paper sizes, format, colorMode, and dpi", () => {
      const valid: Settings = {
        language: "ja",
        imagesToPdf: {
          output: "merge",
          pageSize: "a4",
          orientation: "auto",
          pdfA: false,
          outputDir: null,
        },
        pdfToImages: {
          format: "png",
          colorMode: "color",
          dpi: 150,
          outputDir: null,
        },
      };
      expect(isSettings(valid)).toBe(true);

      // pdfA must be a boolean, and must be there
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, pdfA: true },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, pdfA: "true" },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: {
            output: "merge",
            pageSize: "a4",
            orientation: "auto",
            outputDir: null,
          },
        }),
      ).toBe(false);

      // Accepts all 8 page sizes
      const allSizes: readonly PageSizeChoice[] = [
        "fit",
        "a3",
        "a4",
        "a5",
        "jisB4",
        "jisB5",
        "letter",
        "legal",
      ];
      for (const pageSize of allSizes) {
        expect(
          isSettings({
            ...valid,
            imagesToPdf: { ...valid.imagesToPdf, pageSize },
          }),
        ).toBe(true);
      }
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, pageSize: "b5" },
        }),
      ).toBe(false);

      // Accepts orientations
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, orientation: "portrait" },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, orientation: "landscape" },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, orientation: "square" },
        }),
      ).toBe(false);

      // Rejects missing orientation (including a4Orientation only)
      expect(
        isSettings({
          ...valid,
          imagesToPdf: {
            output: "merge",
            pageSize: "a4",
            outputDir: null,
          },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: {
            output: "merge",
            pageSize: "a4",
            a4Orientation: "auto",
            outputDir: null,
          },
        }),
      ).toBe(false);

      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, format: "webp" },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, format: "gif" },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, colorMode: "gray" },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, colorMode: "sepia" },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, dpi: 36 },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, dpi: 1000 },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, dpi: 35 },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, dpi: 1001 },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          pdfToImages: { ...valid.pdfToImages, dpi: 150.5 },
        }),
      ).toBe(false);
    });

    describe("isPageSizePt", () => {
      it("accepts valid PageSizePt and rejects malformed values", () => {
        expect(isPageSizePt({ widthPt: 595.28, heightPt: 841.89 })).toBe(true);
        expect(isPageSizePt(null)).toBe(false);
        expect(isPageSizePt("string")).toBe(false);
        expect(isPageSizePt({ widthPt: 100 })).toBe(false);
        expect(isPageSizePt({ widthPt: "100", heightPt: 200 })).toBe(false);
        expect(isPageSizePt({ widthPt: Number.NaN, heightPt: 200 })).toBe(
          false,
        );
        expect(
          isPageSizePt({ widthPt: Number.POSITIVE_INFINITY, heightPt: 200 }),
        ).toBe(false);
      });
    });

    describe("isPdfItem", () => {
      const validItem: PdfItem = {
        id: 1,
        name: "test.pdf",
        pageCount: 1,
        firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
        bytes: 1024,
        error: null,
      };

      it("accepts valid PdfItem with or without error and size", () => {
        expect(isPdfItem(validItem)).toBe(true);
        expect(
          isPdfItem({
            ...validItem,
            firstPageSizePt: null,
            error: { code: "PasswordProtected", detail: null },
          }),
        ).toBe(true);
      });

      it("rejects invalid PdfItem shapes", () => {
        expect(isPdfItem(null)).toBe(false);
        expect(isPdfItem({ ...validItem, id: "1" })).toBe(false);
        expect(isPdfItem({ ...validItem, id: 1.5 })).toBe(false);
        expect(isPdfItem({ ...validItem, name: 123 })).toBe(false);
        expect(isPdfItem({ ...validItem, pageCount: "1" })).toBe(false);
        expect(isPdfItem({ ...validItem, bytes: "1024" })).toBe(false);
        expect(
          isPdfItem({ ...validItem, firstPageSizePt: { widthPt: "invalid" } }),
        ).toBe(false);
        expect(
          isPdfItem({
            ...validItem,
            error: { code: "InvalidCode", detail: null },
          }),
        ).toBe(false);
      });
    });

    describe("isUnlockResult", () => {
      it("accepts valid UnlockResult and rejects malformed values", () => {
        expect(isUnlockResult({ items: [] })).toBe(true);
        expect(
          isUnlockResult({
            items: [
              {
                id: 1,
                name: "test.pdf",
                pageCount: 1,
                firstPageSizePt: null,
                bytes: 1024,
                error: null,
              },
            ],
          }),
        ).toBe(true);

        expect(isUnlockResult(null)).toBe(false);
        expect(isUnlockResult({})).toBe(false);
        expect(isUnlockResult({ items: null })).toBe(false);
        expect(isUnlockResult({ items: "not array" })).toBe(false);
        expect(isUnlockResult({ items: [{ id: "bad" }] })).toBe(false);
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
