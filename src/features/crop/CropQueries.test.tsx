import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Candidate, PageImage } from "../../ipc";
import {
  areaRows,
  callsOf,
  candidate,
  emitLayoutPage,
  emitLayoutPages,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  pageImage,
  rowText,
  type Call,
} from "./cropTestSupport";

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

function photoCandidate(imageIndex: number): Candidate {
  return {
    ...candidate("photo", [
      10,
      10 + imageIndex * 100,
      90,
      90 + imageIndex * 100,
    ]),
    imageIndex,
  };
}

async function answer(page: number, candidates: Candidate[]) {
  await emitLayoutPage({ id: 7, page, candidates, error: null });
}

/** Answers of several pages that the screen sees together. */
async function answerTogether(answers: [number, Candidate[]][]) {
  await emitLayoutPages(
    answers.map(([page, candidates]) => ({
      id: 7,
      page,
      candidates,
      error: null,
    })),
  );
}

/**
 * Answers that wait to be released one by one, and note how many were
 * waiting at once: the page that is shown (page 1) is answered at once.
 */
function holdingIpc() {
  const waiting: { cmd: string; page: number; release: () => void }[] = [];
  let atOnce = 0;
  let mostAtOnce = 0;
  const hold = <T,>(cmd: string, page: number, value: T) => {
    atOnce += 1;
    mostAtOnce = Math.max(mostAtOnce, atOnce);
    return new Promise<T>((resolve) => {
      waiting.push({
        cmd,
        page,
        release: () => {
          atOnce -= 1;
          resolve(value);
        },
      });
    });
  };
  return {
    waiting,
    mostAtOnce: () => mostAtOnce,
    handlers: {
      list_page_images: (args: Record<string, unknown>) => {
        const page = Number(args.page);
        const images: PageImage[] = [
          pageImage([10, 10, 90, 90], 0),
          pageImage([10, 110, 90, 190], 1),
        ];
        return page === 1 ? [] : hold("list_page_images", page, images);
      },
      check_on_top: (args: Record<string, unknown>) =>
        hold("check_on_top", Number(args.page), true),
    },
  };
}

async function releaseNext(ipc: ReturnType<typeof holdingIpc>) {
  const next = ipc.waiting.shift();
  expect(next).toBeDefined();
  next?.release();
  await flush();
  return next;
}

function queryCalls(calls: Call[]) {
  return calls
    .filter((c) => c.cmd === "list_page_images" || c.cmd === "check_on_top")
    .map(
      (c) =>
        `${c.cmd}:${c.args.page}${c.cmd === "check_on_top" ? `/${c.args.imageIndex}` : ""}`,
    );
}

describe("the questions the photo areas bring", () => {
  it("asks for the images of each page once, one page at a time, lowest first", async () => {
    const ipc = holdingIpc();
    const calls = await openPdfInCropTab(ipc.handlers);
    expect(queryCalls(calls)).toEqual(["list_page_images:1"]);

    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
    );
    await flush();
    // Answers arrive in the order of Rust's analysis, not of the pages.
    await answerTogether([
      [4, [photoCandidate(0)]],
      [2, [photoCandidate(0)]],
      [3, [photoCandidate(0)]],
    ]);

    expect(queryCalls(calls)).toEqual([
      "list_page_images:1",
      "list_page_images:2",
    ]);

    await releaseNext(ipc);
    expect(queryCalls(calls).slice(2)).toEqual(["list_page_images:3"]);
    await releaseNext(ipc);
    expect(queryCalls(calls).slice(3)).toEqual(["list_page_images:4"]);
    expect(ipc.mostAtOnce()).toBe(1);
  });

  it("asks whether anything lies over each photo, one at a time, in the order of the keys, after the images", async () => {
    const ipc = holdingIpc();
    const calls = await openPdfInCropTab(ipc.handlers);
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
    );
    await flush();
    await answerTogether([
      [3, [photoCandidate(0)]],
      [2, [photoCandidate(1), photoCandidate(0)]],
    ]);

    // The keys are 1 (page 3), 2 and 3 (page 2).
    await releaseNext(ipc);
    expect(queryCalls(calls).slice(1)).toEqual([
      "list_page_images:2",
      "list_page_images:3",
    ]);
    await releaseNext(ipc);
    expect(queryCalls(calls).slice(3)).toEqual(["check_on_top:3/0"]);
    await releaseNext(ipc);
    expect(queryCalls(calls).slice(4)).toEqual(["check_on_top:2/1"]);
    await releaseNext(ipc);
    expect(queryCalls(calls).slice(5)).toEqual(["check_on_top:2/0"]);
    await releaseNext(ipc);

    expect(ipc.mostAtOnce()).toBe(1);
    expect(callsOf(calls, "check_on_top")).toHaveLength(3);
    expect(areaRows().every((row) => rowText(row).includes("重なりあり"))).toBe(
      true,
    );
  });

  it("does not hold up the images of the page that is shown", async () => {
    const ipc = holdingIpc();
    const calls = await openPdfInCropTab(ipc.handlers);
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
    );
    await flush();
    await answer(2, [photoCandidate(0)]);
    expect(queryCalls(calls)).toEqual([
      "list_page_images:1",
      "list_page_images:2",
    ]);

    fireEvent.click(screen.getByRole("button", { name: "ページ 4" }));
    await flush();
    expect(queryCalls(calls)).toContain("list_page_images:4");
  });

  it("asks nothing of a page it has asked about, nor again for an answer it has", async () => {
    const ipc = holdingIpc();
    const calls = await openPdfInCropTab({
      ...ipc.handlers,
      list_page_images: () => [pageImage([10, 10, 90, 90], 0)],
      check_on_top: () => false,
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "ページ全体を範囲にする" }),
    );
    await flush();
    await flush();

    expect(callsOf(calls, "list_page_images")).toHaveLength(1);
    expect(callsOf(calls, "check_on_top")).toHaveLength(1);
  });

  it("does not repeat a question that failed, and puts no mark", async () => {
    let failing = true;
    const calls = await openPdfInCropTab({
      list_page_images: () => [pageImage([10, 10, 90, 90], 0)],
      check_on_top: () => {
        if (failing) {
          throw { code: "WorkerCrashed", detail: null };
        }
        return true;
      },
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(callsOf(calls, "check_on_top")).toHaveLength(1);
    expect(rowText(areaRows()[0])).not.toContain("重なりあり");

    // Other changes do not make it ask again.
    failing = false;
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "ページ全体を範囲にする" }),
    );
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "ページ 2" }));
    await flush();
    expect(callsOf(calls, "check_on_top")).toHaveLength(1);
    expect(rowText(areaRows()[0])).not.toContain("重なりあり");
  });

  it("goes on to the next photo when one question fails", async () => {
    const calls = await openPdfInCropTab({
      list_page_images: () => [
        pageImage([10, 10, 90, 90], 0),
        pageImage([10, 110, 90, 190], 1),
      ],
      check_on_top: (args) => {
        if (args.imageIndex === 0) {
          throw { code: "WorkerTimeout", detail: null };
        }
        return true;
      },
    });
    const [first, second] = screen.getAllByTestId("crop-photo-frame");
    fireEvent.click(first);
    fireEvent.click(second);
    await flush();
    await flush();

    expect(
      callsOf(calls, "check_on_top").map((c) => c.args.imageIndex),
    ).toEqual([0, 1]);
    expect(rowText(areaRows()[0])).not.toContain("重なりあり");
    expect(rowText(areaRows()[1])).toContain("重なりあり");
  });

  it("asks nothing for the areas that are not photos", async () => {
    const calls = await openPdfInCropTab({
      detect_layout: () => [candidate("chart", [10, 10, 90, 90])],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    await flush();
    expect(callsOf(calls, "check_on_top")).toHaveLength(0);
    expect(callsOf(calls, "list_page_images")).toHaveLength(1);
  });

  it("shows the size of a photo on another page once its images have come", async () => {
    const ipc = holdingIpc();
    await openPdfInCropTab(ipc.handlers);
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
    );
    await flush();
    await answer(2, [photoCandidate(0)]);
    expect(rowText(areaRows()[0])).not.toMatch(/元の画素/);

    await releaseNext(ipc);
    expect(rowText(areaRows()[0])).toContain("元の画素 100 × 80");
  });
});
