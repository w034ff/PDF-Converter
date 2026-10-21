import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import type { Candidate, CropPdf, PageImage } from "../../ipc";
import {
  callsOf,
  candidate,
  cropPdf,
  deferred,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  mockCropIpc,
  openCropTab,
  pageImage,
  resizeRoom,
  type Deferred,
  type Handler,
  type ObjectUrls,
} from "./cropTestSupport";

const ROOM_WIDTH = 600;
const ROOM_HEIGHT = 800;

/** The percentage "全体" gives an A4 page in the room above (the height decides). */
const FIT_PAGE_PERCENT = 71;
/** The percentage "幅" gives it (the width decides). */
const FIT_WIDTH_PERCENT = 76;

let urls: ObjectUrls;

beforeEach(() => {
  installFakeResizeObserver(ROOM_WIDTH, ROOM_HEIGHT);
  urls = installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

/** Opens the crop tab on a PDF of `pdf` and waits for what it asks first. */
async function openPdf(
  handlers: Record<string, Handler> = {},
  pdf: CropPdf = cropPdf(),
) {
  const calls = mockCropIpc({ open_crop_pdf: () => pdf, ...handlers });
  render(<App initialNavLang="ja" initialSettings={null} />);
  openCropTab();
  fireEvent.click(screen.getByRole("button", { name: "PDF を開く" }));
  await flush();
  await flush();
  return calls;
}

function pickPage(page: number) {
  fireEvent.click(screen.getByRole("button", { name: `ページ ${page}` }));
}

function percentText(): string {
  return screen.getByTestId("crop-zoom-percent").textContent ?? "";
}

function pageImageCalls(calls: ReturnType<typeof mockCropIpc>) {
  return callsOf(calls, "get_thumbnail").filter(
    (call) => call.args.maxSide !== undefined,
  );
}

describe("the thumbnails", () => {
  it("lists every page and marks the one shown", async () => {
    await openPdf();

    const nav = screen.getByRole("navigation", { name: "ページ" });
    const buttons = within(nav).getAllByRole("button");
    expect(buttons.map((b) => b.getAttribute("aria-label"))).toEqual([
      "ページ 1",
      "ページ 2",
      "ページ 3",
      "ページ 4",
    ]);
    expect(buttons[0]).toHaveAttribute("aria-current", "page");
    expect(buttons[1]).not.toHaveAttribute("aria-current");
  });

  it("asks for the thumbnail of each page at the default size", async () => {
    const calls = await openPdf();

    const thumbnails = callsOf(calls, "get_thumbnail").filter(
      (call) => call.args.maxSide === undefined,
    );
    expect(thumbnails.map((c) => c.args.page).sort()).toEqual([1, 2, 3, 4]);
    expect(thumbnails.every((c) => c.args.id === 7)).toBe(true);
  });

  it("shows the page that is pressed, in the band and by the frame", async () => {
    await openPdf();
    expect(screen.getByText("1 / 4")).toBeInTheDocument();

    pickPage(3);
    await flush();

    expect(screen.getByText("3 / 4")).toBeInTheDocument();
    const nav = screen.getByRole("navigation", { name: "ページ" });
    const [one, , three] = within(nav).getAllByRole("button");
    expect(three).toHaveAttribute("aria-current", "page");
    expect(one).not.toHaveAttribute("aria-current");
    expect(three.querySelector(".crop-thumb-frame")).toHaveClass("is-current");
    expect(one.querySelector(".crop-thumb-frame")).not.toHaveClass(
      "is-current",
    );
  });

  it("keeps the shape of each page", async () => {
    await openPdf(
      {},
      cropPdf({
        pageCount: 2,
        pages: [
          { widthPt: 400, heightPt: 800 },
          { widthPt: 800, heightPt: 400 },
        ],
      }),
    );

    const frames = document.querySelectorAll<HTMLElement>(".crop-thumb-frame");
    expect(frames[0].style.aspectRatio).toBe("400 / 800");
    expect(frames[1].style.aspectRatio).toBe("800 / 400");
  });
});

describe("the page", () => {
  it("is asked for at a step of the size it is shown at", async () => {
    const calls = await openPdf();

    // Fitted to 800 px of height at a device pixel ratio of 1.
    expect(pageImageCalls(calls).map((c) => c.args)).toEqual([
      { id: 7, page: 1, maxSide: 800 },
    ]);
    expect(percentText()).toBe(`${FIT_PAGE_PERCENT}%`);
  });

  it("counts device pixels", async () => {
    vi.stubGlobal("devicePixelRatio", 2);
    const calls = await openPdf();

    // 800 CSS px at twice the density is 1600 device pixels.
    expect(pageImageCalls(calls).map((c) => c.args.maxSide)).toEqual([1600]);
  });

  it("never asks for more than the largest step", async () => {
    const calls = await openPdf();

    for (let i = 0; i < 12; i += 1) {
      fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    }
    await flush();

    expect(percentText()).toBe("400%");
    expect(pageImageCalls(calls).at(-1)?.args.maxSide).toBe(2400);
  });

  it("shows the picture as a blob URL, sized to the page", async () => {
    await openPdf();

    const image = screen.getByRole("img", { name: "report.pdf の 1 ページ" });
    expect(image).toHaveAttribute("src", expect.stringMatching(/^blob:/));
    const page = screen.getByTestId("crop-page");
    // 595 × 842 pt at 71.25 %: 800 px high.
    expect(parseFloat(page.style.height)).toBeCloseTo(800, 0);
    expect(parseFloat(page.style.width)).toBeCloseTo((595 * 800) / 842, 0);
  });

  it("shows the old picture, scaled, until the new one arrives", async () => {
    const second = deferred<number[]>();
    let requests = 0;
    const calls = await openPdf({
      get_thumbnail: (args) => {
        if (args.maxSide === undefined) {
          return [1];
        }
        requests += 1;
        return requests === 1 ? [1] : second.promise;
      },
    });
    const before = screen
      .getByRole("img", { name: /report.pdf/ })
      .getAttribute("src");
    const widthBefore = parseFloat(screen.getByTestId("crop-page").style.width);

    fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    await flush();

    expect(pageImageCalls(calls)).toHaveLength(2);
    expect(screen.getByRole("img", { name: /report.pdf/ })).toHaveAttribute(
      "src",
      before,
    );
    // The page already has its new size.
    expect(
      parseFloat(screen.getByTestId("crop-page").style.width),
    ).toBeGreaterThan(widthBefore);

    second.resolve([2]);
    await flush();

    const after = screen
      .getByRole("img", { name: /report.pdf/ })
      .getAttribute("src");
    expect(after).not.toBe(before);
    expect(urls.revoke).toHaveBeenCalledWith(before);
  });

  it("does not render the page again for a zoom inside the same step", async () => {
    const calls = await openPdf();
    const zoomOut = () =>
      fireEvent.click(screen.getByRole("button", { name: "縮小" }));

    // 71 % is 800 px, 67 % and 50 % are 750 and 561 px: all the 800 step.
    zoomOut();
    zoomOut();
    await flush();
    expect(percentText()).toBe("50%");
    expect(pageImageCalls(calls).map((c) => c.args.maxSide)).toEqual([800]);

    // 33 % is 370 px: the 400 step, and so is 25 % at 280 px.
    zoomOut();
    await flush();
    expect(percentText()).toBe("33%");
    expect(pageImageCalls(calls).map((c) => c.args.maxSide)).toEqual([
      800, 400,
    ]);

    zoomOut();
    await flush();
    expect(percentText()).toBe("25%");
    expect(pageImageCalls(calls).map((c) => c.args.maxSide)).toEqual([
      800, 400,
    ]);
  });

  it("shows nothing of the old page while the next page's picture is on its way", async () => {
    const pageTwo = deferred<number[]>();
    await openPdf({
      get_thumbnail: (args) => {
        if (args.maxSide !== undefined && args.page === 2) {
          return pageTwo.promise;
        }
        return [1];
      },
    });
    expect(screen.getByRole("img", { name: /1 ページ/ })).toBeInTheDocument();

    pickPage(2);
    await flush();

    expect(screen.queryByRole("img", { name: /ページ/ })).toBeNull();
    pageTwo.resolve([1]);
    await flush();
    expect(screen.getByRole("img", { name: /2 ページ/ })).toBeInTheDocument();
  });

  it("releases the pictures when the tab is left", async () => {
    await openPdf();
    const src = screen
      .getByRole("img", { name: /report.pdf/ })
      .getAttribute("src");

    cleanup();

    expect(urls.revoke).toHaveBeenCalledWith(src);
  });
});

describe("the size", () => {
  it("starts with the whole page", async () => {
    await openPdf();
    const group = screen.getByRole("group", { name: "表示の大きさ" });
    expect(within(group).getByRole("button", { name: "全体" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(group).getByRole("button", { name: "幅" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("fits the width", async () => {
    await openPdf();
    fireEvent.click(screen.getByRole("button", { name: "幅" }));

    expect(percentText()).toBe(`${FIT_WIDTH_PERCENT}%`);
    expect(screen.getByRole("button", { name: "幅" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "全体" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("moves to the next step with ＋ and −, and then neither fit is pressed", async () => {
    await openPdf();

    fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    expect(percentText()).toBe("75%");
    expect(screen.getByRole("button", { name: "全体" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "幅" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    expect(percentText()).toBe("90%");
    fireEvent.click(screen.getByRole("button", { name: "縮小" }));
    fireEvent.click(screen.getByRole("button", { name: "縮小" }));
    expect(percentText()).toBe("67%");
  });

  it("stops at the ends of the steps", async () => {
    await openPdf();
    for (let i = 0; i < 15; i += 1) {
      fireEvent.click(screen.getByRole("button", { name: "縮小" }));
    }
    expect(percentText()).toBe("25%");
    expect(screen.getByRole("button", { name: "縮小" })).toBeDisabled();

    for (let i = 0; i < 15; i += 1) {
      fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    }
    expect(percentText()).toBe("400%");
    expect(screen.getByRole("button", { name: "拡大" })).toBeDisabled();
  });

  it("shows 100 % as the actual size", async () => {
    await openPdf();
    for (let i = 0; i < 3; i += 1) {
      fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    }

    expect(percentText()).toBe("100%");
    const page = screen.getByTestId("crop-page");
    expect(parseFloat(page.style.width)).toBeCloseTo((595 * 96) / 72, 1);
    expect(parseFloat(page.style.height)).toBeCloseTo((842 * 96) / 72, 1);
  });

  it("fits again when the room changes, but not a fixed scale", async () => {
    await openPdf();
    expect(percentText()).toBe(`${FIT_PAGE_PERCENT}%`);

    resizeRoom(ROOM_WIDTH, 400);
    // 400 px of height: 400 / (842 · 96 / 72) = 35.6 %.
    expect(percentText()).toBe("36%");

    fireEvent.click(screen.getByRole("button", { name: "幅" }));
    resizeRoom(900, 400);
    expect(percentText()).toBe("113%");

    fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    expect(percentText()).toBe("125%");
    resizeRoom(300, 300);
    expect(percentText()).toBe("125%");
  });

  describe("with Ctrl and the wheel", () => {
    it("moves one step, and stops the browser from zooming the window", async () => {
      await openPdf();
      const viewport = screen.getByTestId("crop-viewport");

      const zoomedIn = fireEvent.wheel(viewport, {
        ctrlKey: true,
        deltaY: -100,
      });
      expect(zoomedIn).toBe(false);
      expect(percentText()).toBe("75%");

      const zoomedOut = fireEvent.wheel(viewport, {
        ctrlKey: true,
        deltaY: 100,
      });
      expect(zoomedOut).toBe(false);
      expect(percentText()).toBe("67%");
      expect(screen.getByRole("button", { name: "全体" })).toHaveAttribute(
        "aria-pressed",
        "false",
      );
    });

    it("moves one step per event even before the screen is drawn again", async () => {
      await openPdf();
      const viewport = screen.getByTestId("crop-viewport");

      act(() => {
        viewport.dispatchEvent(
          new WheelEvent("wheel", {
            ctrlKey: true,
            deltaY: -100,
            cancelable: true,
          }),
        );
        viewport.dispatchEvent(
          new WheelEvent("wheel", {
            ctrlKey: true,
            deltaY: -100,
            cancelable: true,
          }),
        );
      });

      expect(percentText()).toBe("90%");
    });

    it("is stopped at the end of the steps too", async () => {
      await openPdf();
      const viewport = screen.getByTestId("crop-viewport");
      for (let i = 0; i < 15; i += 1) {
        fireEvent.wheel(viewport, { ctrlKey: true, deltaY: -100 });
      }
      expect(percentText()).toBe("400%");

      expect(fireEvent.wheel(viewport, { ctrlKey: true, deltaY: -100 })).toBe(
        false,
      );
      expect(percentText()).toBe("400%");
    });

    it("leaves a plain wheel to scroll the page", async () => {
      await openPdf();
      const viewport = screen.getByTestId("crop-viewport");

      expect(fireEvent.wheel(viewport, { deltaY: -100 })).toBe(true);
      expect(percentText()).toBe(`${FIT_PAGE_PERCENT}%`);
    });
  });
});

describe("the frames and candidates", () => {
  const photos: PageImage[] = [pageImage([100, 200, 300, 400])];
  const candidates: Candidate[] = [
    candidate("table", [50, 60, 250, 160]),
    candidate("figure", [10, 500, 110, 600]),
  ];

  /** Zoomed to `percent` through the steps, from the fitted 71 %. */
  async function zoomTo(percent: number) {
    for (let i = 0; i < 15 && percentText() !== `${percent}%`; i += 1) {
      fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    }
    expect(percentText()).toBe(`${percent}%`);
  }

  it("asks for the images and the candidates of the page shown, once", async () => {
    const calls = await openPdf();

    expect(callsOf(calls, "list_page_images").map((c) => c.args)).toEqual([
      { id: 7, page: 1 },
    ]);
    expect(callsOf(calls, "detect_layout").map((c) => c.args)).toEqual([
      { id: 7, page: 1 },
    ]);

    pickPage(2);
    await flush();
    pickPage(1);
    await flush();
    pickPage(2);
    await flush();

    expect(callsOf(calls, "list_page_images").map((c) => c.args.page)).toEqual([
      1, 2,
    ]);
    expect(callsOf(calls, "detect_layout").map((c) => c.args.page)).toEqual([
      1, 2,
    ]);
  });

  it("draws them where their points are, at the scale of the page", async () => {
    await openPdf({
      list_page_images: () => photos,
      detect_layout: () => candidates,
    });
    await zoomTo(100);

    const frame = screen.getByTestId("crop-photo-frame");
    expect(parseFloat(frame.style.left)).toBeCloseTo((100 * 96) / 72, 1);
    expect(parseFloat(frame.style.top)).toBeCloseTo((200 * 96) / 72, 1);
    expect(parseFloat(frame.style.width)).toBeCloseTo((200 * 96) / 72, 1);
    expect(parseFloat(frame.style.height)).toBeCloseTo((200 * 96) / 72, 1);

    const [table, figure] = screen.getAllByTestId("crop-candidate");
    expect(table).toHaveAttribute("data-kind", "table");
    expect(parseFloat(table.style.left)).toBeCloseTo((50 * 96) / 72, 1);
    expect(parseFloat(table.style.top)).toBeCloseTo((60 * 96) / 72, 1);
    expect(parseFloat(table.style.width)).toBeCloseTo((200 * 96) / 72, 1);
    expect(parseFloat(table.style.height)).toBeCloseTo((100 * 96) / 72, 1);
    expect(figure).toHaveAttribute("data-kind", "figure");

    // At 200 % everything is twice as far and twice as large.
    await zoomTo(200);
    expect(parseFloat(frame.style.left)).toBeCloseTo(((100 * 96) / 72) * 2, 1);
    expect(parseFloat(table.style.width)).toBeCloseTo(((200 * 96) / 72) * 2, 1);
  });

  it("writes the kind on each candidate", async () => {
    await openPdf({
      detect_layout: () => [
        candidate("photo", [0, 0, 10, 10]),
        candidate("figure", [0, 20, 10, 30]),
        candidate("chart", [0, 40, 10, 50]),
        candidate("table", [0, 60, 10, 70]),
      ],
    });

    const tags = screen
      .getAllByTestId("crop-candidate")
      .map((box) => box.textContent);
    expect(tags).toEqual(["写真", "図", "グラフ", "表"]);
  });

  it("hides the candidates, and only them, with the checkbox", async () => {
    await openPdf({
      list_page_images: () => photos,
      detect_layout: () => candidates,
    });
    const checkbox = screen.getByRole("checkbox", { name: "候補" });
    expect(checkbox).toBeChecked();
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);

    fireEvent.click(checkbox);

    expect(checkbox).not.toBeChecked();
    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);
    expect(screen.getAllByTestId("crop-photo-frame")).toHaveLength(1);

    fireEvent.click(checkbox);
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);
  });

  it("shows each page's own frames and candidates", async () => {
    await openPdf({
      list_page_images: (args) => (args.page === 1 ? photos : []),
      detect_layout: (args) => (args.page === 2 ? candidates : []),
    });
    expect(screen.getAllByTestId("crop-photo-frame")).toHaveLength(1);
    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);

    pickPage(2);
    await flush();
    expect(screen.queryAllByTestId("crop-photo-frame")).toHaveLength(0);
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);

    pickPage(1);
    await flush();
    expect(screen.getAllByTestId("crop-photo-frame")).toHaveLength(1);
  });

  it("says the candidates are being looked for until the answer comes", async () => {
    const answer = deferred<Candidate[]>();
    await openPdf({ detect_layout: () => answer.promise });
    expect(screen.getByText("候補を探しています…")).toBeInTheDocument();

    answer.resolve([]);
    await flush();

    expect(screen.queryByText("候補を探しています…")).toBeNull();
  });
});

describe("the analysis one page at a time", () => {
  function heldLayouts() {
    const held = new Map<number, Deferred<Candidate[]>>();
    const handler: Handler = (args) => {
      const page = Number(args.page);
      const answer = deferred<Candidate[]>();
      held.set(page, answer);
      return answer.promise;
    };
    return { held, handler };
  }

  function layoutPages(calls: ReturnType<typeof mockCropIpc>): number[] {
    return callsOf(calls, "detect_layout").map((c) => Number(c.args.page));
  }

  async function settle(answer: Deferred<Candidate[]> | undefined) {
    await act(async () => {
      answer?.resolve([]);
    });
  }

  it("asks only for the last page shown once the answer arrives", async () => {
    const { held, handler } = heldLayouts();
    const calls = await openPdf({ detect_layout: handler });
    expect(layoutPages(calls)).toEqual([1]);

    pickPage(2);
    pickPage(3);
    pickPage(4);
    await flush();
    // Nothing more is sent while page 1 is awaited.
    expect(layoutPages(calls)).toEqual([1]);

    await settle(held.get(1));

    expect(layoutPages(calls)).toEqual([1, 4]);
  });

  it("asks for a page that was skipped when it is shown again", async () => {
    const { held, handler } = heldLayouts();
    const calls = await openPdf({ detect_layout: handler });
    pickPage(2);
    pickPage(3);
    pickPage(4);
    await settle(held.get(1));
    await settle(held.get(4));
    expect(layoutPages(calls)).toEqual([1, 4]);

    pickPage(3);
    await flush();

    expect(layoutPages(calls)).toEqual([1, 4, 3]);
  });

  it("does not hold back the embedded images", async () => {
    const { handler } = heldLayouts();
    const calls = await openPdf({ detect_layout: handler });

    pickPage(2);
    pickPage(3);
    await flush();

    expect(callsOf(calls, "list_page_images").map((c) => c.args.page)).toEqual([
      1, 2, 3,
    ]);
  });

  it("sends one request at a time while pages are shown one after another", async () => {
    let running = 0;
    let most = 0;
    const calls = await openPdf({
      detect_layout: async () => {
        running += 1;
        most = Math.max(most, running);
        await Promise.resolve();
        running -= 1;
        return [];
      },
    });
    for (const page of [2, 3, 4, 1]) {
      pickPage(page);
      await flush();
    }

    expect(most).toBe(1);
    expect(layoutPages(calls)).toEqual([1, 2, 3, 4]);
  });
});

describe("when the analysis fails", () => {
  it("says so above the page for LayoutUnavailable and asks for no other page", async () => {
    const calls = await openPdf({
      detect_layout: () => {
        throw { code: "LayoutUnavailable", detail: null };
      },
    });

    expect(screen.getByRole("status")).toHaveTextContent(
      "候補を表示できません（レイアウトの解析を使えません）。範囲は手で選べます",
    );
    expect(screen.queryByText("候補を探しています…")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();

    pickPage(2);
    pickPage(3);
    await flush();

    expect(callsOf(calls, "detect_layout")).toHaveLength(1);
    // The images are not part of the analysis.
    expect(callsOf(calls, "list_page_images")).toHaveLength(3);
  });

  it("says so in English", async () => {
    mockCropIpc({
      open_crop_pdf: () => cropPdf(),
      detect_layout: () => {
        throw { code: "LayoutUnavailable", detail: null };
      },
    });
    render(<App initialNavLang="en" initialSettings={null} />);
    openCropTab("Crop");
    fireEvent.click(screen.getByRole("button", { name: "Open PDF" }));
    await flush();

    expect(screen.getByRole("status")).toHaveTextContent(
      "Can't show suggestions (layout analysis is unavailable). You can still select areas by hand.",
    );
  });

  it("keeps the notice for the other pages", async () => {
    await openPdf({
      detect_layout: () => {
        throw { code: "LayoutUnavailable", detail: null };
      },
    });
    pickPage(2);
    await flush();
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("puts the failure of one page by that page and goes on with the next", async () => {
    const calls = await openPdf({
      detect_layout: (args) => {
        if (args.page === 1) {
          throw { code: "WorkerTimeout", detail: null };
        }
        return [];
      },
    });

    const message = "PDF の処理に時間がかかりすぎたため中止しました";
    expect(screen.getByText(message)).toBeInTheDocument();
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.queryByText("候補を探しています…")).toBeNull();

    pickPage(2);
    await flush();

    expect(callsOf(calls, "detect_layout").map((c) => c.args.page)).toEqual([
      1, 2,
    ]);
    expect(screen.queryByText(message)).toBeNull();

    pickPage(1);
    await flush();
    expect(screen.getByText(message)).toBeInTheDocument();
    // A failed page is not asked again.
    expect(callsOf(calls, "detect_layout")).toHaveLength(2);
  });

  it("puts the failure to list the images by the page too, leaving the rest usable", async () => {
    await openPdf({
      list_page_images: () => {
        throw { code: "WorkerCrashed", detail: null };
      },
      detect_layout: () => [candidate("table", [0, 0, 50, 50])],
    });

    expect(
      screen.getByText(
        "PDF の処理中に問題が起きました。ファイルが壊れている可能性があります",
      ),
    ).toBeInTheDocument();
    expect(screen.queryAllByTestId("crop-photo-frame")).toHaveLength(0);
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(1);
  });
});

describe("an answer for a PDF that is no longer open", () => {
  it("is dropped", async () => {
    const old = deferred<Candidate[]>();
    const oldImages = deferred<PageImage[]>();
    let opened = 0;
    const calls = await openPdf({
      open_crop_pdf: () => {
        opened += 1;
        return cropPdf({
          id: opened === 1 ? 7 : 8,
          name: `file-${opened}.pdf`,
        });
      },
      list_page_images: (args) => (args.id === 7 ? oldImages.promise : []),
      detect_layout: (args) => (args.id === 7 ? old.promise : []),
    });
    expect(callsOf(calls, "detect_layout").map((c) => c.args.id)).toEqual([7]);

    fireEvent.click(screen.getByRole("button", { name: "別の PDF を開く" }));
    await flush();
    expect(screen.getByText("file-2.pdf")).toBeInTheDocument();
    expect(callsOf(calls, "detect_layout").map((c) => c.args.id)).toEqual([
      7, 8,
    ]);

    await act(async () => {
      old.resolve([candidate("table", [0, 0, 50, 50])]);
      oldImages.resolve([pageImage([0, 0, 50, 50])]);
    });

    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);
    expect(screen.queryAllByTestId("crop-photo-frame")).toHaveLength(0);
    expect(screen.queryByText("候補を探しています…")).toBeNull();
  });

  it("starts the new PDF from its first page, asking for it afresh", async () => {
    let opened = 0;
    const calls = await openPdf({
      open_crop_pdf: () => {
        opened += 1;
        return cropPdf({ id: opened === 1 ? 7 : 8 });
      },
    });
    pickPage(3);
    await flush();

    fireEvent.click(screen.getByRole("button", { name: "別の PDF を開く" }));
    await flush();

    expect(screen.getByText("1 / 4")).toBeInTheDocument();
    expect(
      callsOf(calls, "detect_layout").map((c) => `${c.args.id}:${c.args.page}`),
    ).toEqual(["7:1", "7:3", "8:1"]);
    expect(
      callsOf(calls, "list_page_images").map(
        (c) => `${c.args.id}:${c.args.page}`,
      ),
    ).toEqual(["7:1", "7:3", "8:1"]);
  });
});

describe("while another tab is shown", () => {
  it("asks about no page", async () => {
    const calls = await openPdf();
    const asked = callsOf(calls, "detect_layout").length;

    fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
    await flush();
    openCropTab();
    await flush();

    // Already answered: coming back asks nothing more.
    expect(callsOf(calls, "detect_layout")).toHaveLength(asked);
    expect(screen.getByText("1 / 4")).toBeInTheDocument();
  });

  it("keeps the PDF, the page and the size when the tab is left and shown again", async () => {
    await openPdf();
    pickPage(3);
    fireEvent.click(screen.getByRole("button", { name: "拡大" }));
    await flush();

    fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
    openCropTab();
    await flush();

    expect(screen.getByText("3 / 4")).toBeInTheDocument();
    expect(percentText()).toBe("75%");
  });
});
