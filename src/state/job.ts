import type {
  IpcError,
  JobFinishedPayload,
  JobItemPayload,
  JobItemStatus,
  JobProgressPayload,
} from "../ipc";
import type { ActiveTab } from "./types";

/**
 * Where the one conversion that may run at a time (design §7.1) stands.
 * `cancelling` is entered as soon as Cancel is pressed, before the backend
 * confirms it (design §6.5).
 */
export type JobPhase = "idle" | "running" | "cancelling" | "finished";

/** An item handed to a conversion, in the order it was handed over. */
export interface JobTarget {
  id: number;
  name: string;
}

export interface JobState {
  phase: JobPhase;
  /** The screen that started the last conversion; `null` before any. */
  kind: ActiveTab | null;
  targets: JobTarget[];
  progress: JobProgressPayload | null;
  /** Items whose processing has begun, oldest first. */
  startedIds: number[];
  /** The `job-item` result of each finished item. */
  results: Record<number, JobItemPayload>;
  finished: JobFinishedPayload | null;
  /** The file name `save_merged_pdf` saved, once its command resolves. */
  savedName: string | null;
  error: IpcError | null;
}

export type JobAction =
  | { type: "JOB_STARTED"; kind: ActiveTab; targets: JobTarget[] }
  | { type: "JOB_CANCEL_REQUESTED" }
  | { type: "JOB_PROGRESS"; progress: JobProgressPayload }
  | { type: "JOB_ITEM"; item: JobItemPayload }
  | { type: "JOB_FINISHED"; finished: JobFinishedPayload }
  | { type: "JOB_SAVED"; savedName: string }
  | { type: "JOB_FAILED"; error: IpcError }
  | { type: "JOB_RESET" };

/** How a row of the list reads while or after a conversion runs. */
export type JobRowStatus =
  "waiting" | "running" | "unprocessed" | JobItemStatus;

export const initialJobState: JobState = {
  phase: "idle",
  kind: null,
  targets: [],
  progress: null,
  startedIds: [],
  results: {},
  finished: null,
  savedName: null,
  error: null,
};

/** True while a conversion holds the backend, so lists and settings stay locked (design §6.1). */
export function isJobActive(state: JobState): boolean {
  return state.phase === "running" || state.phase === "cancelling";
}

/**
 * Marks the item that `current` names as started.
 *
 * `job-progress` carries a file name, not an ID, and repeats the PDF's name
 * for each of its pages, so a name that already belongs to a running item
 * starts nothing. Otherwise the first target with that name that has not
 * started is taken: the backend takes items in the order they were handed
 * over. Two running items with the same name show as one.
 */
function startNamedItem(state: JobState, name: string): number[] {
  const running = runningIds(state);
  const nameOf = new Map(state.targets.map((t) => [t.id, t.name]));
  if (running.some((id) => nameOf.get(id) === name)) {
    return state.startedIds;
  }
  const next = state.targets.find(
    (t) =>
      t.name === name &&
      !state.startedIds.includes(t.id) &&
      state.results[t.id] === undefined,
  );
  return next ? [...state.startedIds, next.id] : state.startedIds;
}

export function jobReducer(state: JobState, action: JobAction): JobState {
  switch (action.type) {
    case "JOB_STARTED":
      return {
        ...initialJobState,
        phase: "running",
        kind: action.kind,
        targets: action.targets,
      };
    case "JOB_CANCEL_REQUESTED":
      if (state.phase !== "running") {
        return state;
      }
      return { ...state, phase: "cancelling" };
    case "JOB_PROGRESS": {
      if (!isJobActive(state)) {
        return state;
      }
      const { current } = action.progress;
      return {
        ...state,
        progress: action.progress,
        startedIds:
          current === null ? state.startedIds : startNamedItem(state, current),
      };
    }
    case "JOB_ITEM":
      if (!isJobActive(state)) {
        return state;
      }
      return {
        ...state,
        results: { ...state.results, [action.item.id]: action.item },
      };
    case "JOB_FINISHED":
      if (!isJobActive(state)) {
        return state;
      }
      return { ...state, phase: "finished", finished: action.finished };
    case "JOB_SAVED":
      return { ...state, savedName: action.savedName };
    case "JOB_FAILED":
      // A command that fails after `job-finished` keeps the summary it sent;
      // one that fails before it never ran an item, so no row shows a status.
      if (state.phase === "finished") {
        return { ...state, error: action.error };
      }
      return { ...initialJobState, kind: state.kind, error: action.error };
    case "JOB_RESET":
      return initialJobState;
  }
}

/** Items that have started and not yet reported a result, oldest first. */
export function runningIds(state: JobState): number[] {
  return state.startedIds.filter((id) => state.results[id] === undefined);
}

/**
 * The status to show for item `id`, or `null` if the current or last
 * conversion did not include it.
 */
export function jobRowStatus(state: JobState, id: number): JobRowStatus | null {
  const result = state.results[id];
  if (result !== undefined) {
    return result.status;
  }
  if (!state.targets.some((t) => t.id === id)) {
    return null;
  }
  if (state.phase === "finished") {
    return "unprocessed";
  }
  if (runningIds(state).includes(id)) {
    return "running";
  }
  return "waiting";
}
