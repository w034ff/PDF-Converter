import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { createElement } from "react";
import { App } from "../../App";
import {
  ORGANIZE_DROPPED_EVENT,
  type IpcError,
  type OrganizeDropped,
  type OrganizePdf,
  type Settings,
  type Skipped,
} from "../../ipc";
import {
  DEFAULT_SETTINGS,
  PNG_BYTES,
  flush,
  type Call,
  type Handler,
} from "../crop/cropTestSupport";

/** Helpers shared by the tests of the organize tab; they carry no test of their own. */

export {
  DEFAULT_SETTINGS,
  FINISHED_NONE,
  callsOf,
  deferred,
  emitJobFinished,
  emitJobItem,
  emitJobProgress,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  resizeRoom,
  type Call,
  type Handler,
} from "../crop/cropTestSupport";
import { organizeThumbnails } from "./organizeThumbnails";

export const A4 = { widthPt: 595, heightPt: 842 };
export const LANDSCAPE = { widthPt: 842, heightPt: 595 };

export const PASSWORD: IpcError = { code: "PasswordProtected", detail: null };
export const BROKEN: IpcError = { code: "PdfOpenFailed", detail: null };

export const NO_SKIPPED: Skipped = {
  unsupported: 0,
  folders: 0,
  duplicates: 0,
};

export function organizePdf(
  id: number,
  name: string,
  pageCount = 3,
  overrides: Partial<OrganizePdf> = {},
): OrganizePdf {
  return {
    id,
    name,
    pageCount,
    pages: Array.from({ length: pageCount }, () => A4),
    fileBytes: 1_000_000 * pageCount,
    encrypted: false,
    error: null,
    ...overrides,
  };
}

/** A PDF that waits for its password. */
export function waitingPdf(id: number, name: string): OrganizePdf {
  return organizePdf(id, name, 0, {
    error: PASSWORD,
    encrypted: true,
    fileBytes: 500_000,
  });
}

/** A PDF that could not be read. */
export function brokenPdf(id: number, name: string): OrganizePdf {
  return organizePdf(id, name, 0, { error: BROKEN, fileBytes: 100 });
}

export function added(
  pdfs: OrganizePdf[],
  skipped: Skipped = NO_SKIPPED,
): { added: OrganizePdf[]; skipped: Skipped } {
  return { added: pdfs, skipped };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/**
 * Answers the commands the organize tab sends with the quiet answer of an
 * idle Rust (nothing added, nothing wrong), and what the test gives in
 * `handlers` instead. Every call is recorded.
 */
export function mockOrganizeIpc(
  handlers: Record<string, Handler> = {},
): Call[] {
  const calls: Call[] = [];
  const quiet: Record<string, Handler> = {
    set_drop_target: () => undefined,
    get_thumbnail: () => PNG_BYTES,
    save_settings: () => undefined,
    pick_output_dir: () => null,
    add_organize_pdfs: () => null,
    remove_organize_pdfs: () => undefined,
    save_organized_pdf: () => null,
    start_organize_split: () => undefined,
    cancel_job: () => undefined,
  };
  mockIPC(
    (cmd, payload) => {
      const args = isRecord(payload) ? payload : {};
      calls.push({ cmd, args });
      const handler = handlers[cmd] ?? quiet[cmd];
      if (handler === undefined) {
        return new Promise(() => {});
      }
      return handler(args);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

export interface OpenOptions {
  handlers?: Record<string, Handler>;
  /** The PDFs the first "PDF を追加" brings; none leaves the tab empty. */
  pdfs?: OrganizePdf[];
  settings?: Settings;
  language?: "ja" | "en";
}

/**
 * Renders the app, opens the organize tab and, with `pdfs`, adds them through
 * the dialog. Returns the calls recorded from the start.
 */
export async function openOrganizeTab(
  options: OpenOptions = {},
): Promise<Call[]> {
  const {
    handlers = {},
    pdfs,
    settings = DEFAULT_SETTINGS,
    language = "ja",
  } = options;
  organizeThumbnails.clear();
  const calls = mockOrganizeIpc({
    ...(pdfs === undefined ? {} : { add_organize_pdfs: () => added(pdfs) }),
    ...handlers,
  });
  render(
    createElement(App, { initialNavLang: language, initialSettings: settings }),
  );
  await flush();
  fireEvent.click(
    screen.getByRole("tab", { name: language === "ja" ? "整理" : "Organize" }),
  );
  await flush();
  if (pdfs !== undefined) {
    fireEvent.click(
      screen.getByRole("button", {
        name: language === "ja" ? "PDF を追加" : "Add PDFs",
      }),
    );
    await flush();
  }
  return calls;
}

/** Adds PDFs through the menu of the tab that has some already. */
export async function addThroughMenu() {
  fireEvent.click(screen.getByRole("button", { name: "PDF を追加" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "ファイルを選ぶ" }));
  await flush();
}

export async function emitOrganizeDropped(payload: OrganizeDropped) {
  await act(() => emit(ORGANIZE_DROPPED_EVENT, payload));
}

/** The cells of the grid that are drawn, in the order of the grid. */
export function drawnCells(): HTMLElement[] {
  return screen
    .queryAllByRole("option")
    .filter((option) => option.dataset.pageKey !== undefined);
}

/** The pages `id:page` the grid draws, in the order they are placed. */
export function drawnKeys(): string[] {
  return drawnCells()
    .map((cell) => ({
      key: cell.dataset.pageKey ?? "",
      position: Number(cell.getAttribute("aria-posinset")),
    }))
    .sort((a, b) => a.position - b.position)
    .map((cell) => cell.key);
}

/** The cell of `page` of the PDF with `id`. */
export function cellOf(id: number, page: number): HTMLElement {
  const cell = drawnCells().find(
    (candidate) => candidate.dataset.pageKey === `${id}:${page}`,
  );
  if (cell === undefined) {
    throw new Error(`page ${id}:${page} is not drawn`);
  }
  return cell;
}

export function scrollArea(): HTMLElement {
  const area = screen.getByRole("listbox").parentElement;
  if (area === null) {
    throw new Error("the grid has no scroll area");
  }
  return area;
}

/**
 * Gives the scroll area the size the room has, which jsdom does not lay out:
 * a press is an empty place only inside what is left of the area after its
 * scroll bar.
 */
export function layOutScrollArea(width: number, height: number) {
  const area = scrollArea();
  Object.defineProperty(area, "clientWidth", {
    value: width,
    configurable: true,
  });
  Object.defineProperty(area, "clientHeight", {
    value: height,
    configurable: true,
  });
}

/** Scrolls the grid, which jsdom does not lay out. */
export function scrollGridTo(scrollTop: number) {
  const area = scrollArea();
  area.scrollTop = scrollTop;
  fireEvent.scroll(area);
}

/** A press on a cell, as the primary button, with the keys held. */
export function pressCell(
  cell: HTMLElement,
  init: {
    shiftKey?: boolean;
    ctrlKey?: boolean;
    clientX?: number;
    clientY?: number;
  } = {},
) {
  fireEvent.pointerDown(cell, { button: 0, pointerId: 1, ...init });
}

export function releasePointer(
  init: { clientX?: number; clientY?: number } = {},
) {
  fireEvent.pointerUp(window, { button: 0, pointerId: 1, ...init });
}

export function movePointer(clientX: number, clientY: number) {
  fireEvent.pointerMove(window, { clientX, clientY, pointerId: 1 });
}

/** The pages that are selected, as `id:page`. */
export function selectedKeys(): string[] {
  return drawnCells()
    .filter((cell) => cell.getAttribute("aria-selected") === "true")
    .map((cell) => cell.dataset.pageKey ?? "");
}

/** The settings of an app that saves in parts into a folder already chosen. */
export const SPLIT_SETTINGS: Settings = {
  ...DEFAULT_SETTINGS,
  organize: {
    output: "split",
    encrypt: false,
    outputDir: { dirLabel: "parts" },
  },
};

/** The place between the page `before` and the page `after`, which presses the break before `after`. */
export function breakButton(
  before: number,
  after: number,
  language: "ja" | "en" = "ja",
): HTMLElement {
  return screen.getByRole("button", {
    name:
      language === "ja"
        ? `p.${before} と p.${after} の間の区切り`
        : `Break between p.${before} and p.${after}`,
  });
}

/** The buttons that press a break, and the lines and badges that are drawn. */
export function breakButtons(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(".organize-break-button")];
}

export function breakLines(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(".organize-break-line")];
}

export function badges(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(".organize-part-badge")];
}

/** The calls of `save_organized_pdf`, as the pages they carried. */
export function savedPages(calls: readonly Call[]): unknown[] {
  return calls
    .filter((call) => call.cmd === "save_organized_pdf")
    .map((call) => call.args.pages);
}
