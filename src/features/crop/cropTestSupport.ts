import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { createElement } from "react";
import { vi } from "vitest";
import { App } from "../../App";
import {
  JOB_FINISHED_EVENT,
  JOB_ITEM_EVENT,
  JOB_PROGRESS_EVENT,
  LAYOUT_PAGE_EVENT,
  type JobFinishedPayload,
  type JobItemPayload,
  type JobProgressPayload,
  type LayoutPagePayload,
} from "../../ipc";
import type {
  Candidate,
  CropPdf,
  IpcError,
  PageImage,
  Settings,
} from "../../ipc";

/** Helpers shared by the tests of the crop tab; they carry no test of their own. */

export const A4 = { widthPt: 595, heightPt: 842 };

export const PNG_BYTES = [137, 80, 78, 71];

export function cropPdf(overrides: Partial<CropPdf> = {}): CropPdf {
  const pageCount = overrides.pageCount ?? 4;
  return {
    id: 7,
    name: "report.pdf",
    pageCount,
    pages: Array.from({ length: pageCount }, () => A4),
    error: null,
    ...overrides,
  };
}

/** A pending PDF that could not be opened. */
export function brokenCropPdf(
  id: number,
  error: IpcError,
  name = "broken.pdf",
): CropPdf {
  return { id, name, pageCount: 0, pages: [], error };
}

export function pageImage(
  rectPt: [number, number, number, number],
  imageIndex = 0,
): PageImage {
  return { imageIndex, rectPt, pxWidth: 100, pxHeight: 80, native: true };
}

export function candidate(
  kind: Candidate["kind"],
  rectPt: [number, number, number, number],
): Candidate {
  return { kind, score: 0.9, rectPt, imageIndex: null };
}

export const DEFAULT_SETTINGS: Settings = {
  language: null,
  imagesToPdf: {
    output: "merge",
    pageSize: "fit",
    orientation: "auto",
    pdfA: false,
    encrypt: false,
    outputDir: null,
  },
  pdfToImages: { format: "png", colorMode: "color", dpi: 150, outputDir: null },
  crop: {
    format: "png",
    colorMode: "color",
    dpi: 150,
    photoNative: true,
    photoOnly: false,
    outputDir: null,
  },
};

export interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

export type Handler = (args: Record<string, unknown>) => unknown;

/**
 * Answers the commands the crop tab sends with the quiet answer of an idle
 * Rust (nothing found, nothing wrong), and what the test gives in `handlers`
 * instead. Every call is recorded. A handler may return a promise to hold an
 * answer back, or throw an `IpcError` to reject.
 */
export function mockCropIpc(handlers: Record<string, Handler> = {}): Call[] {
  const calls: Call[] = [];
  const quiet: Record<string, Handler> = {
    open_crop_pdf: () => cropPdf(),
    set_drop_target: () => undefined,
    get_thumbnail: () => PNG_BYTES,
    list_page_images: () => [],
    check_on_top: () => false,
    detect_layout: () => [],
    start_crop_export: () => undefined,
    start_detect_all: () => undefined,
    activate_crop_pdf: () => undefined,
    discard_crop_pdf: () => undefined,
    save_settings: () => undefined,
    pick_output_dir: () => null,
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** The crop options of each `save_settings` call, in order. */
export function savedCrops(calls: readonly Call[]): unknown[] {
  return callsOf(calls, "save_settings").map((call) => {
    const { settings } = call.args;
    return isRecord(settings) ? settings.crop : undefined;
  });
}

export function callsOf(calls: readonly Call[], cmd: string): Call[] {
  return calls.filter((call) => call.cmd === cmd);
}

/** A promise settled from outside, for an answer a test releases when it chooses. */
export interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

export function deferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => {};
  let reject: (reason: unknown) => void = () => {};
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Lets the answers that are ready, and the renders after them, through. */
export async function flush() {
  await act(async () => {});
}

let room = { width: 600, height: 800 };

function entryFor(target: Element): ResizeObserverEntry {
  const { width, height } = room;
  const contentRect: DOMRectReadOnly = {
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: width,
    bottom: height,
    width,
    height,
    toJSON: () => ({}),
  };
  const size: ResizeObserverSize = { inlineSize: width, blockSize: height };
  return {
    target,
    contentRect,
    borderBoxSize: [size],
    contentBoxSize: [size],
    devicePixelContentBoxSize: [size],
  };
}

interface FakeObserver {
  report: () => void;
}

const observers: FakeObserver[] = [];

/** Called with `new`, like the constructor it replaces. */
function FakeResizeObserver(callback: ResizeObserverCallback) {
  let target: Element | null = null;
  const observer: ResizeObserver = {
    observe: (element) => {
      target = element;
      act(() => report());
    },
    unobserve: () => {},
    disconnect: () => {
      target = null;
    },
  };
  function report() {
    if (target !== null) {
      callback([entryFor(target)], observer);
    }
  }
  observers.push({ report });
  return observer;
}

/**
 * Stands in for the browser's `ResizeObserver`, which jsdom does not have.
 * Every observed element is reported to have a content box of `width` ×
 * `height`, at once when it is observed and again on `resizeRoom`.
 */
export function installFakeResizeObserver(width: number, height: number) {
  room = { width, height };
  observers.length = 0;
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
}

export function resizeRoom(width: number, height: number) {
  room = { width, height };
  act(() => {
    for (const observer of observers) {
      observer.report();
    }
  });
}

export interface ObjectUrls {
  create: ReturnType<typeof vi.fn<(blob: Blob) => string>>;
  revoke: ReturnType<typeof vi.fn<(url: string) => void>>;
}

/** Gives the `blob:` URLs of the pictures names a test can follow. */
export function installFakeObjectUrls(): ObjectUrls {
  let next = 0;
  const create = vi.fn<(blob: Blob) => string>(() => `blob:picture-${++next}`);
  const revoke = vi.fn<(url: string) => void>();
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: create,
    revokeObjectURL: revoke,
  });
  return { create, revoke };
}

export function openCropTab(name = "切り出し") {
  fireEvent.click(screen.getByRole("tab", { name }));
}

/**
 * Renders the app, opens the crop tab and a PDF in it (`pdf`, or the one in
 * `handlers`), and lets what the page asks first settle. Returns the calls.
 */
export async function openPdfInCropTab(
  handlers: Record<string, Handler> = {},
  pdf: CropPdf = cropPdf(),
  language: "ja" | "en" = "ja",
): Promise<Call[]> {
  const calls = mockCropIpc({ open_crop_pdf: () => pdf, ...handlers });
  render(
    createElement(App, { initialNavLang: language, initialSettings: null }),
  );
  openCropTab(language === "ja" ? "切り出し" : "Crop");
  fireEvent.click(
    screen.getByRole("button", {
      name: language === "ja" ? "PDF を開く" : "Open PDF",
    }),
  );
  await flush();
  await flush();
  return calls;
}

export async function emitJobProgress(payload: JobProgressPayload) {
  await act(() => emit(JOB_PROGRESS_EVENT, payload));
}

export async function emitJobItem(payload: JobItemPayload) {
  await act(() => emit(JOB_ITEM_EVENT, payload));
}

export async function emitJobFinished(payload: JobFinishedPayload) {
  await act(() => emit(JOB_FINISHED_EVENT, payload));
}

export async function emitLayoutPage(payload: LayoutPagePayload) {
  await act(() => emit(LAYOUT_PAGE_EVENT, payload));
}

/** Several `layout-page` events that arrive before the screen draws again. */
export async function emitLayoutPages(payloads: LayoutPagePayload[]) {
  await act(async () => {
    for (const payload of payloads) {
      await emit(LAYOUT_PAGE_EVENT, payload);
    }
  });
}

export const FINISHED_NONE: JobFinishedPayload = {
  succeeded: 0,
  failed: 0,
  noPages: 0,
  unprocessed: 0,
  cancelled: false,
};

/** The areas' rows of the list, in the order shown. */
export function areaRows(): HTMLElement[] {
  return screen.queryAllByTestId("crop-area-row");
}

/** What a row of the list says, as one line. */
export function rowText(row: HTMLElement): string {
  return (row.textContent ?? "").replace(/\s+/g, " ").trim();
}
