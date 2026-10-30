import { expect } from "vitest";
import {
  keyOfPage,
  organizeReducer,
  type OrganizeAction,
  type OrganizeState,
  type OrganizeStep,
} from "./organize";

/** Helpers shared by the tests of the organize state; they carry no test of their own. */

function expectStepHolds(step: OrganizeStep, where: string) {
  const inGrid = new Set(step.pages.map(keyOfPage));
  const first = step.pages.length > 0 ? keyOfPage(step.pages[0]) : null;
  for (const key of step.breaks) {
    expect(inGrid.has(key), `${where}: break ${key} is no page`).toBe(true);
    expect(key, `${where}: the first page holds a break`).not.toBe(first);
  }
}

/**
 * The invariant of the breaks (organize §4): each is a page of the grid and
 * none is the first. It holds for the grid and for every step of the history,
 * after every action, so this is what every test of the state checks.
 */
export function expectBreaksInvariant(state: OrganizeState) {
  expectStepHolds(state, "the grid");
  state.history.past.forEach((step, index) =>
    expectStepHolds(step, `past ${index}`),
  );
  state.history.future.forEach((step, index) =>
    expectStepHolds(step, `future ${index}`),
  );
}

/** Runs the actions in order, checking the invariant of the breaks after each. */
export function reduceChecked(
  state: OrganizeState,
  ...actions: OrganizeAction[]
): OrganizeState {
  return actions.reduce((current, action) => {
    const next = organizeReducer(current, action);
    expectBreaksInvariant(next);
    return next;
  }, state);
}
