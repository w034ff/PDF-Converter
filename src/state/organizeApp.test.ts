import { describe, expect, it } from "vitest";
import type { OrganizePdf } from "../ipc";
import { appReducer, createInitialAppState } from "./appReducer";
import { initialJobState } from "./job";
import { pageKey } from "./organize";
import type { AppAction, AppState } from "./types";

const A4 = { widthPt: 595, heightPt: 842 };

function pdf(id: number, pageCount: number): OrganizePdf {
  return {
    id,
    name: `file-${id}.pdf`,
    pageCount,
    pages: Array.from({ length: pageCount }, () => A4),
    fileBytes: 1000,
    encrypted: false,
    error: null,
  };
}

function reduce(state: AppState, ...actions: AppAction[]): AppState {
  return actions.reduce(appReducer, state);
}

const FINISHED = {
  succeeded: 1,
  failed: 0,
  noPages: 0,
  unprocessed: 0,
  cancelled: false,
};

const SETTINGS_ORGANIZE = {
  output: "split",
  encrypt: true,
  outputDir: null,
} as const;

/** An app with two PDFs, and the result of a save that finished on the organize tab. */
function afterASave(): AppState {
  return reduce(
    createInitialAppState("ja"),
    { type: "ORGANIZE_PDFS_ADDED", items: [pdf(1, 3), pdf(2, 2)] },
    { type: "JOB_STARTED", kind: "organize", targets: [] },
    { type: "JOB_PROGRESS", progress: { done: 5, total: 5, current: "x.pdf" } },
    {
      type: "JOB_FINISHED",
      finished: {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
    },
    { type: "JOB_SAVED", savedName: "x.pdf" },
  );
}

function running(): AppState {
  return reduce(
    createInitialAppState("ja"),
    { type: "ORGANIZE_PDFS_ADDED", items: [pdf(1, 3)] },
    { type: "JOB_STARTED", kind: "organize", targets: [] },
  );
}

describe("the organize state in the app", () => {
  it("starts from the saved settings, and empty without them", () => {
    const saved = createInitialAppState("ja", {
      language: null,
      imagesToPdf: {
        output: "merge",
        pageSize: "fit",
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
      organize: SETTINGS_ORGANIZE,
    });

    expect(saved.organize.encrypt).toBe(true);
    expect(saved.organize.output).toBe("split");
    expect(saved.organize.pdfs).toEqual([]);
    expect(createInitialAppState("ja").organize.encrypt).toBe(false);
  });

  it("does not share its state between two apps", () => {
    const first = createInitialAppState("ja");
    const second = createInitialAppState("ja");

    expect(first.organize).not.toBe(second.organize);
    expect(first.job).not.toBe(initialJobState);
  });
});

describe("the result of the last save", () => {
  it.each<[string, AppAction]>([
    [
      "a page moved",
      { type: "ORGANIZE_PAGES_MOVED", keys: [pageKey(1, 3)], insertIndex: 0 },
    ],
    [
      "a page turned",
      {
        type: "ORGANIZE_PAGES_ROTATED",
        keys: [pageKey(1, 1)],
        direction: "right",
      },
    ],
    [
      "a page deleted",
      { type: "ORGANIZE_PAGES_DELETED", keys: [pageKey(1, 1)] },
    ],
    ["a PDF added", { type: "ORGANIZE_PDFS_ADDED", items: [pdf(3, 1)] }],
    ["a PDF removed", { type: "ORGANIZE_PDF_REMOVED", id: 2 }],
    ["the PDFs cleared", { type: "ORGANIZE_CLEARED" }],
    [
      "the password setting changed",
      { type: "SET_ORGANIZE_ENCRYPT", encrypt: true },
    ],
    ["a break put in", { type: "ORGANIZE_BREAK_TOGGLED", key: pageKey(1, 2) }],
    ["a break shortcut used", { type: "ORGANIZE_BREAKS_EACH_PAGE" }],
    [
      "the way of saving changed",
      { type: "SET_ORGANIZE_OUTPUT", output: "split" },
    ],
    [
      "the output folder changed",
      { type: "SET_ORGANIZE_OUTPUT_DIR", outputDir: { dirLabel: "out" } },
    ],
  ])("is dropped when %s", (_name, action) => {
    const state = reduce(afterASave(), action);

    expect(state.job).toEqual(initialJobState);
  });

  it("is dropped when an undone step comes back", () => {
    const saved = reduce(
      afterASave(),
      { type: "ORGANIZE_PAGES_DELETED", keys: [pageKey(1, 1)] },
      { type: "JOB_STARTED", kind: "organize", targets: [] },
      { type: "JOB_FINISHED", finished: FINISHED },
    );
    expect(saved.job.finished).not.toBeNull();

    const state = reduce(saved, { type: "ORGANIZE_UNDO" });

    expect(state.job).toEqual(initialJobState);
  });

  it.each<[string, AppAction]>([
    [
      "a page is selected",
      { type: "ORGANIZE_SELECTED_ONLY", key: pageKey(1, 2) },
    ],
    [
      "the selection is extended",
      { type: "ORGANIZE_SELECTION_EXTENDED", key: pageKey(1, 2) },
    ],
    ["every page is selected", { type: "ORGANIZE_SELECTED_ALL" }],
    ["the selection is cleared", { type: "ORGANIZE_SELECTION_CLEARED" }],
    ["the focus moves", { type: "ORGANIZE_FOCUSED", key: pageKey(1, 2) }],
    ["a notice is set", { type: "ORGANIZE_NOTICE", notice: null }],
  ])("stays when %s", (_name, action) => {
    const before = afterASave();

    const state = reduce(before, action);

    expect(state.job).toBe(before.job);
    expect(state.job.finished).not.toBeNull();
  });

  it("stays when the save replaced a PDF, and remembers its name", () => {
    const state = reduce(
      afterASave(),
      { type: "ORGANIZE_PDF_REPLACED", id: 1 },
      { type: "JOB_REPLACED", name: "file-1.pdf" },
    );

    expect(state.organize.pdfs.map((entry) => entry.id)).toEqual([2]);
    expect(state.job.finished).not.toBeNull();
    expect(state.job.savedName).toBe("x.pdf");
    expect(state.job.replacedName).toBe("file-1.pdf");
  });

  it("forgets the name of a replaced PDF with the result", () => {
    const state = reduce(
      afterASave(),
      { type: "JOB_REPLACED", name: "file-1.pdf" },
      { type: "ORGANIZE_PAGES_DELETED", keys: [pageKey(2, 1)] },
    );

    expect(state.job.replacedName).toBeNull();
  });

  it("is not dropped by an action of another tab", () => {
    const before = afterASave();

    const state = reduce(before, {
      type: "SET_IMAGES_OUTPUT_MODE",
      output: "each",
    });

    expect(state.job).toBe(before.job);
  });

  it("is not dropped by the change of another tab's result", () => {
    const before = reduce(afterASave(), {
      type: "SET_ACTIVE_TAB",
      tab: "imagesToPdf",
    });

    expect(before.job.kind).toBe("organize");
  });
});

describe("the way the last conversion saved", () => {
  it("is what it started with, and the setting changing later does not change it", () => {
    const started = reduce(
      createInitialAppState("ja"),
      { type: "ORGANIZE_PDFS_ADDED", items: [pdf(1, 3)] },
      {
        type: "JOB_STARTED",
        kind: "organize",
        targets: [],
        organizeOutput: "split",
      },
    );
    expect(started.job.organizeOutput).toBe("split");

    // A running job keeps its result: the folder is forgotten when Rust says so.
    const forgotten = reduce(started, {
      type: "SET_ORGANIZE_OUTPUT_DIR",
      outputDir: null,
    });
    expect(forgotten.job).toBe(started.job);

    const changed = reduce(started, {
      type: "SET_ORGANIZE_OUTPUT",
      output: "single",
    });
    expect(changed.job.organizeOutput).toBe("split");
  });

  it("is none for another tab's conversion", () => {
    const state = reduce(createInitialAppState("ja"), {
      type: "JOB_STARTED",
      kind: "crop",
      targets: [],
    });

    expect(state.job.organizeOutput).toBeNull();
  });
});

describe("while a conversion runs", () => {
  it("the pages and the breaks cannot be moved, turned, deleted, put in, undone or redone", () => {
    const before = reduce(running(), {
      type: "ORGANIZE_SELECTED_ONLY",
      key: pageKey(1, 1),
    });

    for (const action of [
      { type: "ORGANIZE_BREAK_TOGGLED", key: pageKey(1, 2) },
      { type: "ORGANIZE_BREAKS_EACH_PAGE" },
      { type: "ORGANIZE_BREAKS_EACH_PDF" },
      { type: "ORGANIZE_BREAKS_CLEARED" },
      { type: "ORGANIZE_PAGES_MOVED", keys: [pageKey(1, 1)], insertIndex: 3 },
      {
        type: "ORGANIZE_PAGES_ROTATED",
        keys: [pageKey(1, 1)],
        direction: "left",
      },
      { type: "ORGANIZE_PAGES_DELETED", keys: [pageKey(1, 1)] },
      { type: "ORGANIZE_UNDO" },
      { type: "ORGANIZE_REDO" },
    ] satisfies AppAction[]) {
      expect(appReducer(before, action), action.type).toBe(before);
    }
  });

  it("the selection still works", () => {
    const state = reduce(running(), { type: "ORGANIZE_SELECTED_ALL" });

    expect(state.organize.selected.size).toBe(3);
  });

  it("works again once it has finished", () => {
    const state = reduce(
      running(),
      {
        type: "JOB_FINISHED",
        finished: {
          succeeded: 1,
          failed: 0,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        },
      },
      { type: "ORGANIZE_PAGES_DELETED", keys: [pageKey(1, 1)] },
    );

    expect(state.organize.pages).toHaveLength(2);
  });
});
