import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ImageItem, JobItemPayload } from "../../ipc";
import { AppStateProvider, createInitialAppState } from "../../state";
import { JobFooter } from "./JobFooter";

const multiPageTiff: ImageItem = {
  id: 1,
  name: "scan.tif",
  width: 1728,
  height: 1143,
  format: "tiff",
  pageCount: 3,
  bytes: 3000,
  error: null,
};

const singlePagePng: ImageItem = {
  id: 2,
  name: "photo.png",
  width: 100,
  height: 100,
  format: "png",
  pageCount: 1,
  bytes: 1000,
  error: null,
};

const corruptTiff: ImageItem = {
  id: 3,
  name: "corrupt.tif",
  width: 0,
  height: 0,
  format: null,
  pageCount: 0,
  bytes: 10,
  error: { code: "DecodeFailed", detail: null },
};

function okItem(id: number): JobItemPayload {
  return { id, status: "ok", outputs: [] };
}

function failedItem(id: number): JobItemPayload {
  return {
    id,
    status: "failed",
    outputs: [],
    error: { code: "DecodeFailed", detail: null },
  };
}

describe("JobFooter's summary", () => {
  afterEach(() => {
    cleanup();
  });

  describe("imagesToPdf merged PDF page counts", () => {
    it("sums pageCount across successful items rather than counting succeeded items", () => {
      const state = createInitialAppState("ja-JP");
      state.imagesToPdf.output = "merge";
      state.imagesToPdf.items = [multiPageTiff, singlePagePng];
      state.job = {
        ...state.job,
        kind: "imagesToPdf",
        savedName: "merged.pdf",
        targets: [
          { id: 1, name: "scan.tif" },
          { id: 2, name: "photo.png" },
        ],
        results: {
          1: okItem(1),
          2: okItem(2),
        },
        finished: {
          succeeded: 2,
          failed: 0,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        },
      };

      render(
        <AppStateProvider initialState={state}>
          <JobFooter tab="imagesToPdf" idleStatus={null} action={null} />
        </AppStateProvider>,
      );

      // Total pages is 3 + 1 = 4, while succeeded items is 2.
      expect(
        screen.getByText("変換が終わりました：4 ページの PDF を保存しました"),
      ).toBeInTheDocument();
    });

    it("displays page count sum and failed images count when an item fails", () => {
      const state = createInitialAppState("ja-JP");
      state.imagesToPdf.output = "merge";
      state.imagesToPdf.items = [multiPageTiff, singlePagePng, corruptTiff];
      state.job = {
        ...state.job,
        kind: "imagesToPdf",
        savedName: "merged.pdf",
        targets: [
          { id: 1, name: "scan.tif" },
          { id: 2, name: "photo.png" },
          { id: 3, name: "corrupt.tif" },
        ],
        results: {
          1: okItem(1),
          2: okItem(2),
          3: failedItem(3),
        },
        finished: {
          succeeded: 2,
          failed: 1,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        },
      };

      render(
        <AppStateProvider initialState={state}>
          <JobFooter tab="imagesToPdf" idleStatus={null} action={null} />
        </AppStateProvider>,
      );

      expect(
        screen.getByText(
          "変換が終わりました：4 ページの PDF を保存しました · 失敗 1 枚",
        ),
      ).toBeInTheDocument();
    });

    it("displays in English with singular/plural page count", () => {
      const state = createInitialAppState("en-US");
      state.imagesToPdf.output = "merge";
      state.imagesToPdf.items = [multiPageTiff, singlePagePng];
      state.job = {
        ...state.job,
        kind: "imagesToPdf",
        savedName: "merged.pdf",
        targets: [
          { id: 1, name: "scan.tif" },
          { id: 2, name: "photo.png" },
        ],
        results: {
          1: okItem(1),
          2: okItem(2),
        },
        finished: {
          succeeded: 2,
          failed: 0,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        },
      };

      render(
        <AppStateProvider initialState={state}>
          <JobFooter tab="imagesToPdf" idleStatus={null} action={null} />
        </AppStateProvider>,
      );

      expect(
        screen.getByText("Conversion finished: Saved a 4-page PDF"),
      ).toBeInTheDocument();
    });
  });
});
