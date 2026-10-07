import { describe, expect, it } from "vitest";
import type { JobFinishedPayload, JobItemPayload } from "../ipc";
import {
  initialJobState,
  isJobActive,
  jobReducer,
  jobRowStatus,
  runningIds,
  type JobAction,
  type JobState,
  type JobTarget,
} from "./job";

const targets: JobTarget[] = [
  { id: 1, name: "a.png" },
  { id: 2, name: "b.png" },
  { id: 3, name: "c.png" },
];

const finished: JobFinishedPayload = {
  succeeded: 1,
  failed: 0,
  noPages: 0,
  unprocessed: 2,
  cancelled: true,
};

function okItem(id: number): JobItemPayload {
  return { id, status: "ok", outputs: [`${id}.pdf`] };
}

function progress(done: number, current: string | null): JobAction {
  return { type: "JOB_PROGRESS", progress: { done, total: 3, current } };
}

function reduce(actions: JobAction[], from = initialJobState): JobState {
  return actions.reduce(jobReducer, from);
}

const started: JobAction = {
  type: "JOB_STARTED",
  kind: "imagesToPdf",
  targets,
};

describe("jobReducer", () => {
  it("starts a job afresh, dropping the previous one", () => {
    const previous = reduce([
      started,
      progress(0, "a.png"),
      { type: "JOB_ITEM", item: okItem(1) },
      { type: "JOB_FINISHED", finished },
    ]);
    const state = jobReducer(previous, {
      type: "JOB_STARTED",
      kind: "pdfToImages",
      targets: [{ id: 9, name: "x.pdf" }],
    });

    expect(state).toEqual({
      ...initialJobState,
      phase: "running",
      kind: "pdfToImages",
      targets: [{ id: 9, name: "x.pdf" }],
    });
  });

  it("starts the item that job-progress names", () => {
    const state = reduce([started, progress(0, "a.png"), progress(0, "b.png")]);

    expect(state.progress).toEqual({ done: 0, total: 3, current: "b.png" });
    expect(runningIds(state)).toEqual([1, 2]);
  });

  it("keeps the started items when job-progress names none", () => {
    const state = reduce([started, progress(0, "a.png"), progress(1, null)]);

    expect(runningIds(state)).toEqual([1]);
  });

  it("does not start a second item for a name that is already running", () => {
    // A PDF's name repeats in job-progress for each of its pages.
    const state = reduce([
      started,
      progress(0, "a.png"),
      progress(1, null),
      progress(1, "a.png"),
    ]);

    expect(runningIds(state)).toEqual([1]);
  });

  it("does not take a page of a running PDF for another PDF of the same name", () => {
    const sameName: JobTarget[] = [
      { id: 1, name: "report.pdf" },
      { id: 2, name: "report.pdf" },
    ];
    const state = reduce([
      { type: "JOB_STARTED", kind: "pdfToImages", targets: sameName },
      progress(0, "report.pdf"),
      progress(1, null),
      progress(1, "report.pdf"),
    ]);

    expect(runningIds(state)).toEqual([1]);
  });

  it("starts the next item with the same name once the first has finished", () => {
    const sameName: JobTarget[] = [
      { id: 1, name: "scan.png" },
      { id: 2, name: "scan.png" },
    ];
    const state = reduce([
      { type: "JOB_STARTED", kind: "imagesToPdf", targets: sameName },
      progress(0, "scan.png"),
      { type: "JOB_ITEM", item: okItem(1) },
      progress(1, "scan.png"),
    ]);

    expect(runningIds(state)).toEqual([2]);
  });

  it("ignores a name that matches no target", () => {
    const state = reduce([started, progress(0, "other.png")]);

    expect(runningIds(state)).toEqual([]);
  });

  it("records each item's result and stops counting it as running", () => {
    const failed: JobItemPayload = {
      id: 2,
      status: "failed",
      outputs: [],
      error: { code: "DecodeFailed", detail: null },
    };
    const state = reduce([
      started,
      progress(0, "a.png"),
      progress(0, "b.png"),
      { type: "JOB_ITEM", item: failed },
    ]);

    expect(state.results).toEqual({ 2: failed });
    expect(runningIds(state)).toEqual([1]);
  });

  it("moves to cancelling only while running", () => {
    expect(jobReducer(initialJobState, { type: "JOB_CANCEL_REQUESTED" })).toBe(
      initialJobState,
    );
    const running = reduce([started]);
    const cancelling = jobReducer(running, { type: "JOB_CANCEL_REQUESTED" });
    expect(cancelling.phase).toBe("cancelling");
    expect(isJobActive(cancelling)).toBe(true);

    const done = reduce([{ type: "JOB_FINISHED", finished }], cancelling);
    expect(jobReducer(done, { type: "JOB_CANCEL_REQUESTED" })).toBe(done);
  });

  it("still takes events while cancelling", () => {
    const state = reduce([
      started,
      { type: "JOB_CANCEL_REQUESTED" },
      progress(0, "a.png"),
      { type: "JOB_ITEM", item: okItem(1) },
      { type: "JOB_FINISHED", finished },
    ]);

    expect(state.results[1]).toEqual(okItem(1));
    expect(state.phase).toBe("finished");
    expect(state.finished).toEqual(finished);
  });

  it("ignores events when no job is running", () => {
    const events: JobAction[] = [
      progress(1, "a.png"),
      { type: "JOB_ITEM", item: okItem(1) },
      { type: "JOB_FINISHED", finished },
    ];
    for (const event of events) {
      expect(jobReducer(initialJobState, event)).toBe(initialJobState);
    }

    const done = reduce([started, { type: "JOB_FINISHED", finished }]);
    for (const event of events) {
      expect(jobReducer(done, event)).toBe(done);
    }
  });

  it("records the saved name of a merged PDF", () => {
    const state = reduce([
      started,
      { type: "JOB_FINISHED", finished },
      { type: "JOB_SAVED", savedName: "a.pdf" },
    ]);

    expect(state.savedName).toBe("a.pdf");
    expect(state.finished).toEqual(finished);
  });

  it("drops the job but remembers its screen when the command fails before running", () => {
    const error = { code: "InvalidParams" as const, detail: null };
    const state = reduce([
      started,
      progress(0, "a.png"),
      { type: "JOB_FAILED", error },
    ]);

    expect(state).toEqual({ ...initialJobState, kind: "imagesToPdf", error });
    expect(jobRowStatus(state, 1)).toBeNull();
  });

  it("keeps the summary when the command fails after job-finished", () => {
    const error = { code: "WriteFailed" as const, detail: "disk full" };
    const state = reduce([
      started,
      { type: "JOB_FINISHED", finished },
      { type: "JOB_FAILED", error },
    ]);

    expect(state.phase).toBe("finished");
    expect(state.finished).toEqual(finished);
    expect(state.error).toEqual(error);
  });

  it("records an error when a job could not start without running", () => {
    const error = { code: "InvalidParams" as const, detail: null };
    const previous = reduce([started, { type: "JOB_FINISHED", finished }]);
    const state = jobReducer(previous, {
      type: "JOB_NOT_STARTED",
      kind: "pdfToImages",
      error,
    });

    expect(state).toEqual({
      ...initialJobState,
      kind: "pdfToImages",
      error,
    });
  });

  it("resets to idle", () => {
    const state = reduce([
      started,
      progress(0, "a.png"),
      { type: "JOB_RESET" },
    ]);

    expect(state).toEqual(initialJobState);
  });
});

describe("jobRowStatus", () => {
  it("is null for an item outside the job", () => {
    expect(jobRowStatus(initialJobState, 1)).toBeNull();
    expect(jobRowStatus(reduce([started]), 4)).toBeNull();
  });

  it("tells waiting, running and finished items apart while running", () => {
    const state = reduce([
      started,
      progress(0, "a.png"),
      progress(0, "b.png"),
      { type: "JOB_ITEM", item: okItem(1) },
    ]);

    expect(jobRowStatus(state, 1)).toBe("ok");
    expect(jobRowStatus(state, 2)).toBe("running");
    expect(jobRowStatus(state, 3)).toBe("waiting");
  });

  it("marks items without a result as unprocessed once the job has finished", () => {
    const state = reduce([
      started,
      progress(0, "a.png"),
      progress(0, "b.png"),
      { type: "JOB_ITEM", item: okItem(1) },
      { type: "JOB_FINISHED", finished },
    ]);

    expect(jobRowStatus(state, 1)).toBe("ok");
    expect(jobRowStatus(state, 2)).toBe("unprocessed");
    expect(jobRowStatus(state, 3)).toBe("unprocessed");
  });

  it("passes every job-item status through", () => {
    const statuses = [
      "ok",
      "failed",
      "partial",
      "noPages",
      "cancelled",
    ] as const;
    const state = statuses.reduce(
      (acc, status, index) =>
        jobReducer(acc, {
          type: "JOB_ITEM",
          item: { id: index + 1, status, outputs: [] },
        }),
      reduce([
        {
          type: "JOB_STARTED",
          kind: "pdfToImages",
          targets: statuses.map((_, index) => ({
            id: index + 1,
            name: `${index + 1}.pdf`,
          })),
        },
      ]),
    );

    statuses.forEach((status, index) => {
      expect(jobRowStatus(state, index + 1)).toBe(status);
    });
  });
});
