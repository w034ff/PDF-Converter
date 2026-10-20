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
  pageCount: 1,
  bytes: 12345,
  error: null,
};

const sampleImageItem2: ImageItem = {
  id: 2,
  name: "image2.jpg",
  width: 1920,
  height: 1080,
  format: "jpeg",
  pageCount: 1,
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

    it("updates orientation with SET_IMAGES_ORIENTATION", () => {
      const initial: ImagesToPdfState = initialImagesToPdfState;
      const updated = imagesToPdfReducer(initial, {
        type: "SET_IMAGES_ORIENTATION",
        orientation: "portrait",
      });
      expect(updated.orientation).toBe("portrait");

      const unchanged = imagesToPdfReducer(updated, {
        type: "SET_IMAGES_ORIENTATION",
        orientation: "portrait",
      });
      expect(unchanged).toBe(updated);
    });

    it("updates pdfA with SET_IMAGES_PDF_A, starting from false", () => {
      expect(initialImagesToPdfState.pdfA).toBe(false);
      const updated = imagesToPdfReducer(initialImagesToPdfState, {
        type: "SET_IMAGES_PDF_A",
        pdfA: true,
      });
      expect(updated.pdfA).toBe(true);

      const unchanged = imagesToPdfReducer(updated, {
        type: "SET_IMAGES_PDF_A",
        pdfA: true,
      });
      expect(unchanged).toBe(updated);
    });

    it("updates encrypt with SET_IMAGES_ENCRYPT, starting from false", () => {
      expect(initialImagesToPdfState.encrypt).toBe(false);
      const updated = imagesToPdfReducer(initialImagesToPdfState, {
        type: "SET_IMAGES_ENCRYPT",
        encrypt: true,
      });
      expect(updated.encrypt).toBe(true);
      // The two boxes are independent in the state: the screen keeps them apart.
      expect(updated.pdfA).toBe(false);

      const unchanged = imagesToPdfReducer(updated, {
        type: "SET_IMAGES_ENCRYPT",
        encrypt: true,
      });
      expect(unchanged).toBe(updated);
    });

    it("retains orientation when pageSize changes", () => {
      const initial: ImagesToPdfState = {
        ...initialImagesToPdfState,
        pageSize: "a4",
        orientation: "landscape",
      };
      const switchedToB5 = imagesToPdfReducer(initial, {
        type: "SET_IMAGES_PAGE_SIZE",
        pageSize: "jisB5",
      });
      expect(switchedToB5.pageSize).toBe("jisB5");
      expect(switchedToB5.orientation).toBe("landscape");

      const switchedToFit = imagesToPdfReducer(switchedToB5, {
        type: "SET_IMAGES_PAGE_SIZE",
        pageSize: "fit",
      });
      expect(switchedToFit.pageSize).toBe("fit");
      expect(switchedToFit.orientation).toBe("landscape");

      const switchedBackToA4 = imagesToPdfReducer(switchedToFit, {
        type: "SET_IMAGES_PAGE_SIZE",
        pageSize: "a4",
      });
      expect(switchedBackToA4.pageSize).toBe("a4");
      expect(switchedBackToA4.orientation).toBe("landscape");
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

    it("restores orientation, colorMode, and custom DPI from settings in createInitialAppState", () => {
      const statePreset = createInitialAppState("ja-JP", {
        language: "ja",
        imagesToPdf: {
          output: "merge",
          pageSize: "a4",
          orientation: "portrait",
          pdfA: true,
          encrypt: false,
          outputDir: null,
        },
        pdfToImages: {
          format: "png",
          colorMode: "gray",
          dpi: 300,
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
      });
      expect(statePreset.imagesToPdf.orientation).toBe("portrait");
      expect(statePreset.imagesToPdf.pdfA).toBe(true);
      expect(statePreset.imagesToPdf.encrypt).toBe(false);
      expect(statePreset.pdfToImages.colorMode).toBe("gray");
      expect(statePreset.pdfToImages.dpi).toBe(300);
      expect(statePreset.pdfToImages.isCustomDpi).toBe(false);
      expect(statePreset.pdfToImages.customDpiText).toBe("300");

      const stateCustom = createInitialAppState("ja-JP", {
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
          format: "webp",
          colorMode: "color",
          dpi: 600,
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
      });
      expect(stateCustom.pdfToImages.dpi).toBe(600);
      expect(stateCustom.pdfToImages.isCustomDpi).toBe(true);
      expect(stateCustom.pdfToImages.customDpiText).toBe("600");
      expect(stateCustom.pdfToImages.format).toBe("webp");
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

      it("drops it when colorMode of pdfToImages changes", () => {
        const state = appReducer(finishedOn("pdfToImages"), {
          type: "SET_RENDER_COLOR_MODE",
          colorMode: "gray",
        });
        expect(state.job.finished).toBeNull();
      });

      it("drops it when custom DPI of pdfToImages changes", () => {
        const state = appReducer(finishedOn("pdfToImages"), {
          type: "SET_CUSTOM_DPI_TEXT",
          text: "600",
        });
        expect(state.job.finished).toBeNull();
      });

      it("drops it when REPLACE_PDF_ITEMS changes items in pdfToImages", () => {
        const base = finishedOn("pdfToImages");
        const unlockedPdf: PdfItem = {
          ...samplePdfItem1,
          pageCount: 10,
        };
        const state = appReducer(base, {
          type: "REPLACE_PDF_ITEMS",
          items: [unlockedPdf],
        });
        expect(state.job.finished).toBeNull();
      });

      it("drops it when orientation of imagesToPdf changes", () => {
        const state = appReducer(finishedOn("imagesToPdf"), {
          type: "SET_IMAGES_ORIENTATION",
          orientation: "landscape",
        });
        expect(state.job.finished).toBeNull();
      });

      it("drops it when the PDF/A choice of imagesToPdf changes", () => {
        const state = appReducer(finishedOn("imagesToPdf"), {
          type: "SET_IMAGES_PDF_A",
          pdfA: true,
        });
        expect(state.job.finished).toBeNull();
      });

      it("drops it when the password choice of imagesToPdf changes", () => {
        const state = appReducer(finishedOn("imagesToPdf"), {
          type: "SET_IMAGES_ENCRYPT",
          encrypt: true,
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
          result: { totalPages: 1, intervals: [[1, 1]], lastPage: false },
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
