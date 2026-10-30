import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  addImages,
  activateCropPdf,
  addOrganizePdfs,
  addPdfs,
  cancelJob,
  checkOnTop,
  checkPageRange,
  CROP_DROPPED_EVENT,
  detectLayout,
  discardCropPdf,
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
  LAYOUT_PAGE_EVENT,
  listPageImages,
  normalizeIpcError,
  onCropDropped,
  onItemsDropped,
  onJobFinished,
  onJobItem,
  onJobProgress,
  onLayoutPage,
  onOrganizeDropped,
  openCropPdf,
  ORGANIZE_DROPPED_EVENT,
  pickOutputDir,
  removeItems,
  removeOrganizePdfs,
  saveMergedPdf,
  saveOrganizedPdf,
  saveSettings,
  setDropTarget,
  startCropExport,
  startDetectAll,
  startImagesToPdfs,
  startOrganizeSplit,
  startPdfsToImages,
  unlockCropPdf,
  unlockOrganizePdf,
  unlockPdf,
  type AddResult,
  type Candidate,
  type CropDropped,
  type CropPdf,
  type CropRegionInput,
  type ImageItem,
  type ItemsDropped,
  type JobFinishedPayload,
  type JobItemPayload,
  type JobProgressPayload,
  type LayoutPagePayload,
  type OrganizeAdded,
  type OrganizeDropped,
  type OrganizePageInput,
  type OrganizePdf,
  type OrganizeUnlocked,
  type PageImage,
  type PageSizeChoice,
  type PdfItem,
  type Settings,
  type SettingsInput,
  type UnlockResult,
} from ".";

const NO_SKIPS = { unsupported: 0, folders: 0, duplicates: 0 };

const ORGANIZE_PDF: OrganizePdf = {
  id: 5,
  name: "a.pdf",
  pageCount: 2,
  pages: [
    { widthPt: 595.28, heightPt: 841.89 },
    { widthPt: 841.89, heightPt: 595.28 },
  ],
  fileBytes: 12345,
  encrypted: false,
  error: null,
};

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
            pageCount: 1,
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

    it("sends maxSide to get_thumbnail only when it is given", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return new ArrayBuffer(4);
      });

      await getThumbnail(5, 2, 1200);
      expect(calls).toEqual([
        ["get_thumbnail", { id: 5, page: 2, maxSide: 1200 }],
      ]);
    });

    it("asks open_crop_pdf for the pending PDF and returns null when cancelled", async () => {
      const cropPdf: CropPdf = {
        id: 7,
        name: "a.pdf",
        pageCount: 1,
        pages: [{ widthPt: 595.28, heightPt: 841.89 }],
        error: null,
      };
      const calls: unknown[] = [];
      let answer: CropPdf | null = cropPdf;
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return answer;
      });

      expect(await openCropPdf()).toEqual(cropPdf);
      answer = null;
      expect(await openCropPdf()).toBeNull();
      expect(calls).toEqual([
        ["open_crop_pdf", {}],
        ["open_crop_pdf", {}],
      ]);
    });

    it("sends only the ID to activate_crop_pdf and discard_crop_pdf", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await activateCropPdf(3);
      await discardCropPdf(4);
      expect(calls).toEqual([
        ["activate_crop_pdf", { id: 3 }],
        ["discard_crop_pdf", { id: 4 }],
      ]);
    });

    it("sends id and password to unlock_crop_pdf and returns the PDF", async () => {
      const cropPdf: CropPdf = {
        id: 3,
        name: "encrypted.pdf",
        pageCount: 2,
        pages: [
          { widthPt: 595.28, heightPt: 841.89 },
          { widthPt: 595.28, heightPt: 841.89 },
        ],
        error: null,
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return cropPdf;
      });

      expect(await unlockCropPdf(3, "secret password")).toEqual(cropPdf);
      expect(calls).toEqual([
        ["unlock_crop_pdf", { id: 3, password: "secret password" }],
      ]);
    });

    it("sends the target to set_drop_target", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await setDropTarget("crop");
      await setDropTarget("lists");
      await setDropTarget("organize");
      expect(calls).toEqual([
        ["set_drop_target", { target: "crop" }],
        ["set_drop_target", { target: "lists" }],
        ["set_drop_target", { target: "organize" }],
      ]);
    });

    it("asks list_page_images for the page and returns the images", async () => {
      const images: PageImage[] = [
        {
          imageIndex: 0,
          rectPt: [10, 20, 110, 220],
          pxWidth: 400,
          pxHeight: 800,
          nativeSize: { width: 400, height: 800, scaledDown: false },
        },
      ];
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return images;
      });

      expect(await listPageImages(3, 2)).toEqual(images);
      expect(calls).toEqual([["list_page_images", { id: 3, page: 2 }]]);
    });

    it("sends imageIndex in camelCase to check_on_top", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return false;
      });

      expect(await checkOnTop(3, 2, 5)).toBe(false);
      expect(calls).toEqual([
        ["check_on_top", { id: 3, page: 2, imageIndex: 5 }],
      ]);
    });

    it("asks detect_layout for the page and returns the candidates", async () => {
      const candidates: Candidate[] = [
        {
          kind: "photo",
          score: 0.9,
          rectPt: [10, 20, 110, 220],
          imageIndex: 0,
        },
        {
          kind: "chart",
          score: 0.8,
          rectPt: [10, 300, 110, 400],
          imageIndex: null,
        },
      ];
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return candidates;
      });

      expect(await detectLayout(3, 1)).toEqual(candidates);
      expect(calls).toEqual([["detect_layout", { id: 3, page: 1 }]]);
    });

    it("rejects detect_layout with LayoutUnavailable as Rust sent it", async () => {
      mockIPC(() => {
        throw { code: "LayoutUnavailable", detail: "model missing" };
      });

      await expect(detectLayout(3, 1)).rejects.toEqual({
        code: "LayoutUnavailable",
        detail: "model missing",
      });
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
          lastPage: false,
        };
      });

      const res = await checkPageRange("1-3, 5", [1, 2]);
      expect(res).toEqual({
        totalPages: 5,
        intervals: [
          [1, 3],
          [5, 5],
        ],
        lastPage: false,
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
          encrypt: false,
          outputDir: { dirLabel: "out" },
        },
        pdfToImages: {
          format: "png",
          colorMode: "color",
          dpi: 150,
          outputDir: null,
        },
        crop: {
          format: "png",
          colorMode: "color",
          dpi: 150,
          photoNative: true,
          photoOnly: false,
          outputDir: null,
        },
        organize: { output: "single", encrypt: false, outputDir: null },
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
          encrypt: false,
        },
        pdfToImages: { format: "jpeg", colorMode: "color", dpi: 300 },
        crop: {
          format: "webp",
          colorMode: "gray",
          dpi: 72,
          photoNative: false,
          photoOnly: true,
        },
        organize: { output: "split", encrypt: true },
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
            encrypt: false,
          },
          pdfToImages: { format: "png", colorMode: "color", dpi: 100 },
          crop: {
            format: "png",
            colorMode: "color",
            dpi: 150,
            photoNative: true,
            photoOnly: false,
          },
          organize: { output: "single", encrypt: false },
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
            encrypt: false,
          },
          pdfToImages: { format: "png", colorMode: "color", dpi: 150 },
          crop: {
            format: "png",
            colorMode: "color",
            dpi: 150,
            photoNative: true,
            photoOnly: false,
          },
          organize: { output: "single", encrypt: false },
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

    it("asks pick_output_dir for the crop folder", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return { dirLabel: "crops" };
      });

      expect(await pickOutputDir("crop")).toEqual({ dirLabel: "crops" });
      expect(calls).toEqual([["pick_output_dir", { kind: "crop" }]]);
    });

    it("asks pick_output_dir for the organize folder", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return { dirLabel: "organized" };
      });

      expect(await pickOutputDir("organize")).toEqual({
        dirLabel: "organized",
      });
      expect(calls).toEqual([["pick_output_dir", { kind: "organize" }]]);
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

      const res = await saveMergedPdf([1, 2], "fit", "auto", true, null);
      expect(res).toEqual({ savedName: "out.pdf" });
      expect(calls).toEqual([
        [
          "save_merged_pdf",
          {
            ids: [1, 2],
            pageSize: "fit",
            orientation: "auto",
            pdfA: true,
            password: null,
          },
        ],
      ]);
    });

    it("tells a cancelled dialog from a merge that wrote no PDF", async () => {
      mockIPC(() => null);
      expect(await saveMergedPdf([1], "fit", "auto", false, null)).toBeNull();

      clearMocks();
      mockIPC(() => ({ savedName: null }));
      expect(await saveMergedPdf([1], "fit", "auto", false, null)).toEqual({
        savedName: null,
      });
    });

    it("calls startImagesToPdfs", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startImagesToPdfs([1, 2], "a4", "portrait", true, null);
      expect(calls).toEqual([
        [
          "start_images_to_pdfs",
          {
            ids: [1, 2],
            pageSize: "a4",
            orientation: "portrait",
            pdfA: true,
            password: null,
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

  describe("crop commands", () => {
    it("calls startDetectAll with the id", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startDetectAll(3);
      expect(calls).toEqual([["start_detect_all", { id: 3 }]]);
    });

    it("rejects startDetectAll with the { code, detail } Rust returned", async () => {
      mockIPC(() => {
        throw { code: "ConversionRunning", detail: null };
      });

      await expect(startDetectAll(3)).rejects.toEqual({
        code: "ConversionRunning",
        detail: null,
      });
    });

    it("calls startCropExport with camelCase arguments and the ranges as they are", async () => {
      const regions: CropRegionInput[] = [
        { key: 1, page: 1, rectPt: [10, 20, 110, 220], imageIndex: 0 },
        { key: 2, page: 2, rectPt: [0, 0, 50, 60], imageIndex: null },
      ];
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startCropExport(3, regions, "webp", "gray", 300, false, true);
      expect(calls).toEqual([
        [
          "start_crop_export",
          {
            id: 3,
            regions,
            format: "webp",
            colorMode: "gray",
            dpi: 300,
            photoNative: false,
            photoOnly: true,
          },
        ],
      ]);
    });

    it("rejects startCropExport with InvalidParams as Rust returned it", async () => {
      mockIPC(() => {
        throw { code: "InvalidParams", detail: null };
      });

      await expect(
        startCropExport(3, [], "png", "color", 150, true, false),
      ).rejects.toEqual({ code: "InvalidParams", detail: null });
    });
  });

  describe("organize commands", () => {
    it("asks add_organize_pdfs for the source and returns null when cancelled", async () => {
      const added: OrganizeAdded = {
        added: [ORGANIZE_PDF],
        skipped: { unsupported: 1, folders: 0, duplicates: 2 },
      };
      const calls: unknown[] = [];
      let answer: OrganizeAdded | null = added;
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return answer;
      });

      expect(await addOrganizePdfs("files")).toEqual(added);
      answer = null;
      expect(await addOrganizePdfs("folder")).toBeNull();
      expect(calls).toEqual([
        ["add_organize_pdfs", { source: "files" }],
        ["add_organize_pdfs", { source: "folder" }],
      ]);
    });

    it("sends only IDs to remove_organize_pdfs", async () => {
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await removeOrganizePdfs([5, 6]);
      expect(calls).toEqual([["remove_organize_pdfs", { ids: [5, 6] }]]);
    });

    it("sends id and password to unlock_organize_pdf and returns the PDFs", async () => {
      const unlocked: OrganizeUnlocked = {
        items: [{ ...ORGANIZE_PDF, encrypted: true }],
      };
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return unlocked;
      });

      expect(await unlockOrganizePdf(5, "secret")).toEqual(unlocked);
      expect(calls).toEqual([
        ["unlock_organize_pdf", { id: 5, password: "secret" }],
      ]);
    });

    it("rejects unlock_organize_pdf with the PasswordProtected Rust returned", async () => {
      mockIPC(() => {
        throw { code: "PasswordProtected", detail: null };
      });

      await expect(unlockOrganizePdf(5, "wrong")).rejects.toEqual({
        code: "PasswordProtected",
        detail: null,
      });
    });

    it("sends the pages and the password to save_organized_pdf", async () => {
      const pages: OrganizePageInput[] = [
        { id: 5, page: 2, rotation: 90 },
        { id: 6, page: 1, rotation: 0 },
      ];
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return { savedName: "a.pdf", replacedId: 4 };
      });

      expect(await saveOrganizedPdf(pages, "secret")).toEqual({
        savedName: "a.pdf",
        replacedId: 4,
      });
      expect(calls).toEqual([
        ["save_organized_pdf", { pages, password: "secret" }],
      ]);
    });

    it("tells a cancelled dialog from an organize that saved no PDF", async () => {
      mockIPC(() => null);
      expect(await saveOrganizedPdf([], null)).toBeNull();

      clearMocks();
      mockIPC(() => ({ savedName: null, replacedId: null }));
      expect(await saveOrganizedPdf([], null)).toEqual({
        savedName: null,
        replacedId: null,
      });
    });

    it("rejects save_organized_pdf with the OutputTooLarge Rust returned", async () => {
      mockIPC(() => {
        throw { code: "OutputTooLarge", detail: "613 MB / 512 MB" };
      });

      await expect(saveOrganizedPdf([], null)).rejects.toEqual({
        code: "OutputTooLarge",
        detail: "613 MB / 512 MB",
      });
    });

    it("sends the parts and the password to start_organize_split", async () => {
      const parts: OrganizePageInput[][] = [
        [{ id: 5, page: 1, rotation: 0 }],
        [
          { id: 5, page: 2, rotation: 180 },
          { id: 6, page: 3, rotation: 270 },
        ],
      ];
      const calls: unknown[] = [];
      mockIPC((cmd, args) => {
        calls.push([cmd, args]);
        return undefined;
      });

      await startOrganizeSplit(parts, null);
      expect(calls).toEqual([
        ["start_organize_split", { parts, password: null }],
      ]);
    });

    it("rejects start_organize_split with InvalidParams as Rust returned it", async () => {
      mockIPC(() => {
        throw { code: "InvalidParams", detail: null };
      });

      await expect(startOrganizeSplit([], null)).rejects.toEqual({
        code: "InvalidParams",
        detail: null,
      });
    });
  });

  describe("errors", () => {
    it("knows the codes of design §6.6", () => {
      expect(isErrorCode("PasswordProtected")).toBe(true);
      expect(isErrorCode("ConversionRunning")).toBe(true);
      expect(isErrorCode("LayoutUnavailable")).toBe(true);
      expect(isErrorCode("OutputTooLarge")).toBe(true);
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
          encrypt: false,
          outputDir: null,
        },
        pdfToImages: {
          format: "png",
          colorMode: "color",
          dpi: 150,
          outputDir: null,
        },
        crop: {
          format: "png",
          colorMode: "color",
          dpi: 150,
          photoNative: true,
          photoOnly: false,
          outputDir: null,
        },
        organize: { output: "single", encrypt: false, outputDir: null },
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

      // encrypt must be a boolean, and must be there
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, encrypt: true },
        }),
      ).toBe(true);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: { ...valid.imagesToPdf, encrypt: "true" },
        }),
      ).toBe(false);
      expect(
        isSettings({
          ...valid,
          imagesToPdf: {
            output: "merge",
            pageSize: "a4",
            orientation: "auto",
            pdfA: false,
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

      // The crop part is checked as deeply as the others, and must be there
      const withCrop = (change: Record<string, unknown>) => ({
        ...valid,
        crop: { ...valid.crop, ...change },
      });
      expect(isSettings(withCrop({ format: "webp", dpi: 1000 }))).toBe(true);
      expect(isSettings(withCrop({ colorMode: "gray", dpi: 36 }))).toBe(true);
      expect(
        isSettings(withCrop({ photoNative: false, photoOnly: true })),
      ).toBe(true);
      expect(isSettings(withCrop({ outputDir: { dirLabel: "crops" } }))).toBe(
        true,
      );
      expect(isSettings({ ...valid, crop: undefined })).toBe(false);
      expect(isSettings({ ...valid, crop: null })).toBe(false);
      expect(isSettings(withCrop({ format: "gif" }))).toBe(false);
      expect(isSettings(withCrop({ colorMode: "sepia" }))).toBe(false);
      expect(isSettings(withCrop({ dpi: 35 }))).toBe(false);
      expect(isSettings(withCrop({ dpi: 1001 }))).toBe(false);
      expect(isSettings(withCrop({ dpi: 150.5 }))).toBe(false);
      expect(isSettings(withCrop({ photoNative: "true" }))).toBe(false);
      expect(isSettings(withCrop({ photoOnly: 0 }))).toBe(false);
      expect(isSettings(withCrop({ outputDir: "crops" }))).toBe(false);

      // The organize part must be there and is checked too
      const withOrganize = (change: Record<string, unknown>) => ({
        ...valid,
        organize: { ...valid.organize, ...change },
      });
      expect(isSettings(withOrganize({ output: "split" }))).toBe(true);
      expect(isSettings(withOrganize({ encrypt: true }))).toBe(true);
      expect(isSettings(withOrganize({ outputDir: { dirLabel: "out" } }))).toBe(
        true,
      );
      expect(isSettings({ ...valid, organize: undefined })).toBe(false);
      expect(isSettings({ ...valid, organize: null })).toBe(false);
      expect(isSettings(withOrganize({ output: "merge" }))).toBe(false);
      expect(isSettings(withOrganize({ encrypt: "true" }))).toBe(false);
      expect(isSettings(withOrganize({ outputDir: "out" }))).toBe(false);
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

    it("passes the payload of crop-dropped to the handler", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: CropDropped[] = [];
      const unlisten = await onCropDropped((payload) => {
        received.push(payload);
      });
      const payload: CropDropped = {
        cropPdf: null,
        notOnePdf: true,
        error: null,
      };

      await emit(CROP_DROPPED_EVENT, payload);
      unlisten();
      await emit(CROP_DROPPED_EVENT, payload);

      expect(received).toEqual([payload]);
    });

    it("passes the payload of organize-dropped to the handler", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: OrganizeDropped[] = [];
      const unlisten = await onOrganizeDropped((payload) => {
        received.push(payload);
      });
      const payload: OrganizeDropped = {
        added: [ORGANIZE_PDF],
        skipped: { unsupported: 1, folders: 1, duplicates: 0 },
        error: null,
      };

      await emit(ORGANIZE_DROPPED_EVENT, payload);
      unlisten();
      await emit(ORGANIZE_DROPPED_EVENT, payload);

      expect(received).toEqual([payload]);
    });

    it("keeps organize-dropped apart from items-dropped", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: ItemsDropped[] = [];
      const unlisten = await onItemsDropped((payload) => {
        received.push(payload);
      });

      await emit(ORGANIZE_DROPPED_EVENT, {
        added: [],
        skipped: NO_SKIPS,
        error: null,
      });
      unlisten();

      expect(received).toEqual([]);
    });

    it("keeps crop-dropped apart from items-dropped", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: ItemsDropped[] = [];
      const unlisten = await onItemsDropped((payload) => {
        received.push(payload);
      });

      await emit(CROP_DROPPED_EVENT, {
        cropPdf: null,
        notOnePdf: true,
        error: null,
      });
      unlisten();

      expect(received).toEqual([]);
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

    it("passes sourceId of a job-item that failed on a PDF", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: JobItemPayload[] = [];
      const unlisten = await onJobItem((payload) => {
        received.push(payload);
      });
      const payload: JobItemPayload = {
        id: 2,
        status: "failed",
        outputs: [],
        error: { code: "PdfOpenFailed", detail: null },
        sourceId: 5,
      };

      await emit(JOB_ITEM_EVENT, payload);
      unlisten();

      expect(received).toEqual([payload]);
      expect(received[0].sourceId).toBe(5);
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

    it("passes the payload of layout-page to the handler", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: LayoutPagePayload[] = [];
      const unlisten = await onLayoutPage((payload) => {
        received.push(payload);
      });
      const found: LayoutPagePayload = {
        id: 3,
        page: 1,
        candidates: [
          {
            kind: "chart",
            score: 0.8,
            rectPt: [10, 300, 110, 400],
            imageIndex: null,
          },
        ],
        error: null,
      };
      const failed: LayoutPagePayload = {
        id: 3,
        page: 2,
        candidates: [],
        error: { code: "WorkerTimeout", detail: null },
      };

      await emit(LAYOUT_PAGE_EVENT, found);
      await emit(LAYOUT_PAGE_EVENT, failed);
      unlisten();
      await emit(LAYOUT_PAGE_EVENT, found);

      expect(received).toEqual([found, failed]);
    });

    it("keeps layout-page apart from job-item", async () => {
      mockIPC(() => undefined, { shouldMockEvents: true });
      const received: JobItemPayload[] = [];
      const unlisten = await onJobItem((payload) => {
        received.push(payload);
      });

      await emit(LAYOUT_PAGE_EVENT, {
        id: 3,
        page: 1,
        candidates: [],
        error: null,
      });
      unlisten();

      expect(received).toEqual([]);
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
