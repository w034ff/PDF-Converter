import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  addImages,
  addPdfs,
  getThumbnail,
  isErrorCode,
  isIpcError,
  ITEMS_DROPPED_EVENT,
  normalizeIpcError,
  onItemsDropped,
  removeItems,
  type AddResult,
  type ImageItem,
  type ItemsDropped,
  type PdfItem,
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
      };

      await emit(ITEMS_DROPPED_EVENT, payload);
      unlisten();
      await emit(ITEMS_DROPPED_EVENT, payload);

      expect(received).toEqual([payload]);
    });
  });
});
