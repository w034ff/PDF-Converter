import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ja, en } from "../../i18n";
import type { ImageItem } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  type ImagesToPdfState,
} from "../../state";
import { getImagesToPdfStatusText } from "./imagesToPdfStatusText";
import { ImagesToPdfStatus } from "./ImagesToPdfStatus";

const validItem1: ImageItem = {
  id: 1,
  name: "photo.jpg",
  width: 100,
  height: 100,
  format: "jpeg",
  bytes: 1000,
  error: null,
};

const validItem2: ImageItem = {
  id: 2,
  name: "diagram.png",
  width: 200,
  height: 200,
  format: "png",
  bytes: 2000,
  error: null,
};

const corruptItem: ImageItem = {
  id: 3,
  name: "broken.png",
  width: 0,
  height: 0,
  format: null,
  bytes: 10,
  error: { code: "DecodeFailed", detail: null },
};

describe("ImagesToPdfStatus", () => {
  afterEach(() => {
    cleanup();
  });

  describe("getImagesToPdfStatusText (pure function)", () => {
    it("returns 'no images selected' when list is empty", () => {
      const state: ImagesToPdfState = {
        items: [],
        output: "merge",
        pageSize: "fit",
        a4Orientation: "auto",
        outputDir: null,
      };
      expect(getImagesToPdfStatusText(state, ja)).toBe(
        "画像が選ばれていません",
      );
      expect(getImagesToPdfStatusText(state, en)).toBe("No images selected");
    });

    it("asks to remove unreadable images when all items have errors", () => {
      const state: ImagesToPdfState = {
        items: [corruptItem],
        output: "merge",
        pageSize: "fit",
        a4Orientation: "auto",
        outputDir: null,
      };
      expect(getImagesToPdfStatusText(state, ja)).toBe(
        "変換できる画像がありません。読み込めない画像を外してください",
      );
      expect(getImagesToPdfStatusText(state, en)).toBe(
        "No images can be converted. Remove the ones that couldn't be loaded.",
      );
    });

    describe("single PDF mode (merge)", () => {
      it("formats 1-page PDF correctly in Japanese and English singular", () => {
        const state: ImagesToPdfState = {
          items: [validItem1],
          output: "merge",
          pageSize: "fit",
          a4Orientation: "auto",
          outputDir: null,
        };
        expect(getImagesToPdfStatusText(state, ja)).toBe(
          "1 ページの PDF になります",
        );
        expect(getImagesToPdfStatusText(state, en)).toBe(
          "Will create a 1-page PDF",
        );
      });

      it("formats multi-page PDF correctly in Japanese and English", () => {
        const state: ImagesToPdfState = {
          items: [validItem1, validItem2],
          output: "merge",
          pageSize: "fit",
          a4Orientation: "auto",
          outputDir: null,
        };
        expect(getImagesToPdfStatusText(state, ja)).toBe(
          "2 ページの PDF になります",
        );
        expect(getImagesToPdfStatusText(state, en)).toBe(
          "Will create a 2-page PDF",
        );
      });

      it("excludes error items from page count", () => {
        const state: ImagesToPdfState = {
          items: [validItem1, corruptItem, validItem2],
          output: "merge",
          pageSize: "fit",
          a4Orientation: "auto",
          outputDir: null,
        };
        expect(getImagesToPdfStatusText(state, ja)).toBe(
          "2 ページの PDF になります",
        );
        expect(getImagesToPdfStatusText(state, en)).toBe(
          "Will create a 2-page PDF",
        );
      });
    });

    describe("each image mode", () => {
      it("formats single PDF in Japanese and English singular", () => {
        const state: ImagesToPdfState = {
          items: [validItem1],
          output: "each",
          pageSize: "fit",
          a4Orientation: "auto",
          outputDir: null,
        };
        expect(getImagesToPdfStatusText(state, ja)).toBe(
          "1 個の PDF を保存します",
        );
        expect(getImagesToPdfStatusText(state, en)).toBe("Will save 1 PDF");
      });

      it("formats multiple PDFs in Japanese and English plural", () => {
        const state: ImagesToPdfState = {
          items: [validItem1, validItem2],
          output: "each",
          pageSize: "fit",
          a4Orientation: "auto",
          outputDir: null,
        };
        expect(getImagesToPdfStatusText(state, ja)).toBe(
          "2 個の PDF を保存します",
        );
        expect(getImagesToPdfStatusText(state, en)).toBe("Will save 2 PDFs");
      });

      it("excludes error items from PDF count", () => {
        const state: ImagesToPdfState = {
          items: [validItem1, corruptItem],
          output: "each",
          pageSize: "fit",
          a4Orientation: "auto",
          outputDir: null,
        };
        expect(getImagesToPdfStatusText(state, ja)).toBe(
          "1 個の PDF を保存します",
        );
        expect(getImagesToPdfStatusText(state, en)).toBe("Will save 1 PDF");
      });
    });
  });

  describe("ImagesToPdfStatus component", () => {
    it("renders hint class with text from app state", () => {
      const initialState = createInitialAppState("ja-JP");
      initialState.imagesToPdf.items = [validItem1];
      initialState.imagesToPdf.output = "merge";

      render(
        <AppStateProvider initialState={initialState}>
          <ImagesToPdfStatus />
        </AppStateProvider>,
      );

      const span = screen.getByText("1 ページの PDF になります");
      expect(span).toBeInTheDocument();
      expect(span).toHaveClass("hint");
    });

    it("renders the unreadable-only line as a warning", () => {
      const initialState = createInitialAppState("ja-JP");
      initialState.imagesToPdf.items = [corruptItem];

      render(
        <AppStateProvider initialState={initialState}>
          <ImagesToPdfStatus />
        </AppStateProvider>,
      );

      expect(
        screen.getByText(
          "変換できる画像がありません。読み込めない画像を外してください",
        ),
      ).toHaveClass("error-text");
    });

    it("keeps the empty-list line plain", () => {
      const initialState = createInitialAppState("ja-JP");

      render(
        <AppStateProvider initialState={initialState}>
          <ImagesToPdfStatus />
        </AppStateProvider>,
      );

      expect(screen.getByText("画像が選ばれていません")).toHaveClass("hint");
    });
  });
});
