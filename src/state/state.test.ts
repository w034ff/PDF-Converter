import { describe, expect, it } from "vitest";
import type { ImageItem, PdfItem } from "../ipc";
import {
  appReducer,
  createInitialAppState,
  imagesToPdfReducer,
  initialImagesToPdfState,
  initialPdfToImagesState,
  languageReducer,
  pdfToImagesReducer,
  type ImagesToPdfState,
  type LanguageState,
  type PdfToImagesState,
} from "./index";

const sampleImageItem1: ImageItem = {
  id: 1,
  name: "image1.png",
  width: 800,
  height: 600,
  format: "png",
  bytes: 12345,
  error: null,
};

const sampleImageItem2: ImageItem = {
  id: 2,
  name: "image2.jpg",
  width: 1920,
  height: 1080,
  format: "jpeg",
  bytes: 54321,
  error: null,
};

const samplePdfItem1: PdfItem = {
  id: 10,
  name: "document.pdf",
  pageCount: 5,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 99999,
  error: null,
};

const samplePdfItem2: PdfItem = {
  id: 11,
  name: "report.pdf",
  pageCount: 1,
  firstPageSizePt: { widthPt: 612, heightPt: 792 },
  bytes: 88888,
  error: null,
};

describe("state reducers", () => {
  describe("languageReducer", () => {
    it("updates language with SET_LANGUAGE", () => {
      const initial: LanguageState = {
        language: "ja",
        preference: "ja",
        activeTab: "imagesToPdf",
      };
      const updated = languageReducer(initial, {
        type: "SET_LANGUAGE",
        language: "en",
      });
      expect(updated.language).toBe("en");
      expect(updated.preference).toBe("en");
      expect(updated.activeTab).toBe("imagesToPdf");
    });

    it("records picking the language already shown from the OS", () => {
      const initial: LanguageState = {
        language: "ja",
        preference: null,
        activeTab: "imagesToPdf",
      };
      const updated = languageReducer(initial, {
        type: "SET_LANGUAGE",
        language: "ja",
      });
      expect(updated.preference).toBe("ja");
    });

    it("returns same state when language does not change", () => {
      const initial: LanguageState = {
        language: "ja",
        preference: "ja",
        activeTab: "imagesToPdf",
      };
      const updated = languageReducer(initial, {
        type: "SET_LANGUAGE",
        language: "ja",
      });
      expect(updated).toBe(initial);
    });

    it("updates active tab with SET_ACTIVE_TAB", () => {
      const initial: LanguageState = {
        language: "ja",
        preference: "ja",
        activeTab: "imagesToPdf",
      };
      const updated = languageReducer(initial, {
        type: "SET_ACTIVE_TAB",
        tab: "pdfToImages",
      });
      expect(updated.activeTab).toBe("pdfToImages");
      expect(updated.language).toBe("ja");
    });

    it("returns same state when active tab does not change", () => {
      const initial: LanguageState = {
        language: "ja",
        preference: "ja",
        activeTab: "imagesToPdf",
      };
      const updated = languageReducer(initial, {
        type: "SET_ACTIVE_TAB",
        tab: "imagesToPdf",
      });
      expect(updated).toBe(initial);
    });
  });

  describe("imagesToPdfReducer", () => {
    it("adds image items with ADD_IMAGE_ITEMS", () => {
      const initial: ImagesToPdfState = initialImagesToPdfState;
      const updated = imagesToPdfReducer(initial, {
        type: "ADD_IMAGE_ITEMS",
        items: [sampleImageItem1, sampleImageItem2],
      });
      expect(updated.items).toHaveLength(2);
      expect(updated.items[0]).toEqual(sampleImageItem1);
      expect(updated.items[1]).toEqual(sampleImageItem2);
    });

    it("returns same state when ADD_IMAGE_ITEMS is given empty list", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        items: [sampleImageItem1],
      };
      const updated = imagesToPdfReducer(initial, {
        type: "ADD_IMAGE_ITEMS",
        items: [],
      });
      expect(updated).toBe(initial);
    });

    it("removes image item by id with REMOVE_IMAGE_ITEM", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        items: [sampleImageItem1, sampleImageItem2],
      };
      const updated = imagesToPdfReducer(initial, {
        type: "REMOVE_IMAGE_ITEM",
        id: 1,
      });
      expect(updated.items).toHaveLength(1);
      expect(updated.items[0]).toEqual(sampleImageItem2);
    });

    it("returns same state when REMOVE_IMAGE_ITEM id does not exist", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        items: [sampleImageItem1],
      };
      const updated = imagesToPdfReducer(initial, {
        type: "REMOVE_IMAGE_ITEM",
        id: 999,
      });
      expect(updated).toBe(initial);
    });

    it("clears all items with CLEAR_IMAGE_ITEMS", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        items: [sampleImageItem1, sampleImageItem2],
      };
      const updated = imagesToPdfReducer(initial, {
        type: "CLEAR_IMAGE_ITEMS",
      });
      expect(updated.items).toHaveLength(0);
    });

    it("updates output mode with SET_IMAGES_OUTPUT_MODE", () => {
      const initial: ImagesToPdfState = initialImagesToPdfState;
      const updated = imagesToPdfReducer(initial, {
        type: "SET_IMAGES_OUTPUT_MODE",
        output: "each",
      });
      expect(updated.output).toBe("each");

      // No-op if same output
      const unchanged = imagesToPdfReducer(updated, {
        type: "SET_IMAGES_OUTPUT_MODE",
        output: "each",
      });
      expect(unchanged).toBe(updated);
    });

    it("updates page size with SET_IMAGES_PAGE_SIZE", () => {
      const initial: ImagesToPdfState = initialImagesToPdfState;
      const updated = imagesToPdfReducer(initial, {
        type: "SET_IMAGES_PAGE_SIZE",
        pageSize: "a4",
      });
      expect(updated.pageSize).toBe("a4");

      // No-op if same pageSize
      const unchanged = imagesToPdfReducer(updated, {
        type: "SET_IMAGES_PAGE_SIZE",
        pageSize: "a4",
      });
      expect(unchanged).toBe(updated);
    });

    it("updates output dir with SET_IMAGES_OUTPUT_DIR", () => {
      const initial: ImagesToPdfState = initialImagesToPdfState;
      const updated = imagesToPdfReducer(initial, {
        type: "SET_IMAGES_OUTPUT_DIR",
        outputDir: { dirLabel: "output-folder" },
      });
      expect(updated.outputDir).toEqual({ dirLabel: "output-folder" });
    });

    it("reorders items with MOVE_IMAGE_ITEM", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        items: [sampleImageItem1, sampleImageItem2],
      };
      const updated = imagesToPdfReducer(initial, {
        type: "MOVE_IMAGE_ITEM",
        fromIndex: 0,
        toIndex: 1,
      });
      expect(updated.items[0]).toEqual(sampleImageItem2);
      expect(updated.items[1]).toEqual(sampleImageItem1);
    });

    it("ignores invalid MOVE_IMAGE_ITEM ranges", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        items: [sampleImageItem1, sampleImageItem2],
      };
      expect(
        imagesToPdfReducer(initial, {
          type: "MOVE_IMAGE_ITEM",
          fromIndex: -1,
          toIndex: 1,
        }),
      ).toBe(initial);
      expect(
        imagesToPdfReducer(initial, {
          type: "MOVE_IMAGE_ITEM",
          fromIndex: 0,
          toIndex: 5,
        }),
      ).toBe(initial);
      expect(
        imagesToPdfReducer(initial, {
          type: "MOVE_IMAGE_ITEM",
          fromIndex: 0,
          toIndex: 0,
        }),
      ).toBe(initial);
    });
  });

  describe("pdfToImagesReducer", () => {
    it("adds pdf items with ADD_PDF_ITEMS", () => {
      const initial: PdfToImagesState = {
        ...initialPdfToImagesState,
        items: [],
      };
      const updated = pdfToImagesReducer(initial, {
        type: "ADD_PDF_ITEMS",
        items: [samplePdfItem1, samplePdfItem2],
      });
      expect(updated.items).toHaveLength(2);
      expect(updated.items[0]).toEqual(samplePdfItem1);
      expect(updated.items[1]).toEqual(samplePdfItem2);
    });

    it("removes pdf item by id with REMOVE_PDF_ITEM", () => {
      const initial: PdfToImagesState = {
        ...initialPdfToImagesState,
        items: [samplePdfItem1, samplePdfItem2],
      };
      const updated = pdfToImagesReducer(initial, {
        type: "REMOVE_PDF_ITEM",
        id: 10,
      });
      expect(updated.items).toHaveLength(1);
      expect(updated.items[0]).toEqual(samplePdfItem2);
    });

    it("clears all items with CLEAR_PDF_ITEMS", () => {
      const initial: PdfToImagesState = {
        ...initialPdfToImagesState,
        items: [samplePdfItem1, samplePdfItem2],
      };
      const updated = pdfToImagesReducer(initial, {
        type: "CLEAR_PDF_ITEMS",
      });
      expect(updated.items).toHaveLength(0);
    });
  });

  describe("appReducer", () => {
    it("initializes default app state with ja language", () => {
      const state = createInitialAppState("ja-JP");
      expect(state.language.language).toBe("ja");
      expect(state.language.activeTab).toBe("imagesToPdf");
      expect(state.imagesToPdf.items).toHaveLength(0);
      expect(state.pdfToImages.items).toHaveLength(0);
    });

    it("dispatches actions to appropriate sub-reducers", () => {
      let state = createInitialAppState("ja-JP");

      state = appReducer(state, {
        type: "SET_ACTIVE_TAB",
        tab: "pdfToImages",
      });
      expect(state.language.activeTab).toBe("pdfToImages");

      state = appReducer(state, {
        type: "ADD_PDF_ITEMS",
        items: [samplePdfItem1],
      });
      expect(state.pdfToImages.items).toHaveLength(1);

      state = appReducer(state, {
        type: "ADD_IMAGE_ITEMS",
        items: [sampleImageItem1],
      });
      expect(state.imagesToPdf.items).toHaveLength(1);
    });

    describe("dropping a stale result", () => {
      const finished = {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      };

      function finishedOn(tab: "imagesToPdf" | "pdfToImages") {
        const state = createInitialAppState("ja-JP");
        return {
          ...state,
          imagesToPdf: { ...state.imagesToPdf, items: [sampleImageItem1] },
          pdfToImages: { ...state.pdfToImages, items: [samplePdfItem1] },
          job: {
            ...state.job,
            phase: "finished" as const,
            kind: tab,
            targets: [{ id: 1, name: "x" }],
            finished,
          },
        };
      }

      it("drops it when the list of its screen changes", () => {
        const state = appReducer(finishedOn("imagesToPdf"), {
          type: "ADD_IMAGE_ITEMS",
          items: [sampleImageItem2],
        });
        expect(state.job.finished).toBeNull();
        expect(state.job.kind).toBeNull();
      });

      it("drops it when a setting of its screen changes", () => {
        const state = appReducer(finishedOn("pdfToImages"), {
          type: "SET_RENDER_DPI",
          dpi: 300,
        });
        expect(state.job.finished).toBeNull();
      });

      it("drops an error that kept a job from starting", () => {
        const base = finishedOn("pdfToImages");
        const state = appReducer(
          {
            ...base,
            job: {
              ...base.job,
              phase: "idle",
              finished: null,
              error: { code: "WriteFailed", detail: null },
            },
          },
          { type: "SET_RENDER_FORMAT", format: "jpeg" },
        );
        expect(state.job.error).toBeNull();
      });

      it("keeps it when the other screen changes", () => {
        const state = appReducer(finishedOn("pdfToImages"), {
          type: "SET_IMAGES_PAGE_SIZE",
          pageSize: "a4",
        });
        expect(state.job.finished).toEqual(finished);
      });

      it("keeps it when nothing changes", () => {
        const base = finishedOn("imagesToPdf");
        const state = appReducer(base, {
          type: "SET_IMAGES_OUTPUT_MODE",
          output: base.imagesToPdf.output,
        });
        expect(state).toBe(base);
      });

      it("keeps it for the answer of a range check", () => {
        const state = appReducer(finishedOn("pdfToImages"), {
          type: "CHECK_RANGE_SUCCESS",
          result: { totalPages: 1, intervals: [[1, 1]] },
        });
        expect(state.job.finished).toEqual(finished);
      });

      it("keeps a running job", () => {
        const base = finishedOn("imagesToPdf");
        const state = appReducer(
          { ...base, job: { ...base.job, phase: "running", finished: null } },
          { type: "ADD_IMAGE_ITEMS", items: [sampleImageItem2] },
        );
        expect(state.job.phase).toBe("running");
      });
    });
  });
});
