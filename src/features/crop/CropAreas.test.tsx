import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  areaRows,
  callsOf,
  candidate,
  cropPdf,
  drawWholePage,
  expectPageShown,
  toggleCandidates,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  openSettings,
  pageImage,
  pageHeads,
  rowPage,
  rowText,
  type ObjectUrls,
} from "./cropTestSupport";

let urls: ObjectUrls;

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  urls = installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const PHOTO_RECT: [number, number, number, number] = [100, 200, 300, 400];
const photos = [pageImage(PHOTO_RECT, 2)];

function pickPage(page: number) {
  fireEvent.click(screen.getByRole("button", { name: `ページ ${page}` }));
}

async function addMenuItem(name: string, menu = "範囲を追加") {
  fireEvent.click(screen.getByRole("button", { name: menu }));
  fireEvent.click(screen.getByRole("menuitem", { name }));
  await flush();
}

async function addWholePage() {
  await drawWholePage();
}

/** The candidate of `kind`; the page stacks them by size, not in their order. */
function candidateOf(kind: string): HTMLElement {
  const found = screen
    .getAllByTestId("crop-candidate")
    .find((element) => element.dataset.kind === kind);
  if (found === undefined) {
    throw new Error(`no ${kind} candidate`);
  }
  return found;
}

function selectedAreas(): HTMLElement[] {
  return screen
    .queryAllByTestId("crop-area")
    .filter((area) => area.dataset.selected === "true");
}

describe("pressing a frame", () => {
  it("adds a photo area with its image, selects it and asks whether anything lies on it", async () => {
    const calls = await openPdfInCropTab({ list_page_images: () => photos });
    expect(areaRows()).toHaveLength(0);

    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();

    const [row] = areaRows();
    expect(rowPage(row)).toBe("p.1");
    expect(rowText(row)).toContain("写真");
    expect(rowText(row)).toContain("元の画素 100 × 80");
    expect(row).toHaveAttribute("data-key", "1");
    expect(selectedAreas()).toHaveLength(1);
    expect(
      within(row).getByRole("button", { name: "p.1 の写真を表示" }),
    ).toHaveAttribute("aria-current", "true");
    expect(callsOf(calls, "check_on_top").map((c) => c.args)).toEqual([
      { id: 7, page: 1, imageIndex: 2 },
    ]);
    expect(
      screen.getByText("1 個の範囲を PNG で書き出します"),
    ).toBeInTheDocument();
  });

  it("marks a photo with something over it", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      check_on_top: () => true,
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(rowText(areaRows()[0])).toContain("重なりあり");
    expect(within(areaRows()[0]).getByText("重なりあり")).toHaveAttribute(
      "title",
      "写真の上に文字・図形・ほかの画像などが重なっています。「写真だけを書き出す」で除けます",
    );
  });

  it("says the photo is written without what lies over it while only photos are written", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      check_on_top: () => true,
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    openSettings();
    const only = screen.getByRole("checkbox", { name: "写真だけを書き出す" });

    fireEvent.click(only);
    expect(rowText(areaRows()[0])).not.toContain("重なりあり");
    expect(within(areaRows()[0]).getByText("重なりを除く")).toHaveAttribute(
      "title",
      "写真の上に重なった文字・図形・ほかの画像などを除いて書き出します",
    );

    fireEvent.click(only);
    expect(rowText(areaRows()[0])).toContain("重なりあり");
    expect(rowText(areaRows()[0])).not.toContain("重なりを除く");
  });

  it("marks nothing when nothing lies over the photo, or when the question failed", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      check_on_top: () => false,
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(rowText(areaRows()[0])).not.toContain("重なりあり");
  });

  it("makes a photo area of a frame that cannot be written as it is, and sizes it by the resolution", async () => {
    await openPdfInCropTab({
      list_page_images: () => [{ ...photos[0], nativeSize: null }],
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();

    expect(rowText(areaRows()[0])).toContain("写真");
    expect(rowText(areaRows()[0])).toContain("150 dpi · 417 × 417");
  });

  it("draws the area where the frame was", async () => {
    await openPdfInCropTab({ list_page_images: () => photos });
    const frame = screen.getByTestId("crop-photo-frame");
    fireEvent.click(frame);
    const area = screen.getByTestId("crop-area");
    expect(area.style.left).toBe(frame.style.left);
    expect(area.style.width).toBe(frame.style.width);
    expect(area).toHaveTextContent("写真");
  });

  it("adds nothing when the frame is pressed twice, and keeps it selected", async () => {
    await openPdfInCropTab({ list_page_images: () => photos });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    fireEvent.click(screen.getByTestId("crop-area"));
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();

    expect(areaRows()).toHaveLength(1);
    expect(selectedAreas()).toHaveLength(1);
  });
});

describe("pressing a candidate", () => {
  const candidates = [
    candidate("chart", [50, 60, 250, 160]),
    { ...candidate("photo", PHOTO_RECT), imageIndex: 2 },
  ];

  it("adds an area of the candidate's kind", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      detect_layout: () => candidates,
    });
    fireEvent.click(candidateOf("chart"));
    await flush();

    const [row] = areaRows();
    expect(rowText(row)).toContain("グラフ");
    expect(rowText(row)).toContain("150 dpi · 417 × 208");
    expect(screen.getByTestId("crop-area")).toHaveTextContent("グラフ");
    expect(selectedAreas()).toHaveLength(1);
  });

  it("leaves an image a photo candidate stands for to the candidate, and draws its frame while the candidates are hidden", async () => {
    const calls = await openPdfInCropTab({
      list_page_images: () => photos,
      detect_layout: () => candidates,
    });
    expect(screen.queryAllByTestId("crop-photo-frame")).toHaveLength(0);
    fireEvent.click(candidateOf("photo"));
    await flush();
    expect(callsOf(calls, "check_on_top")).toHaveLength(1);

    toggleCandidates();
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();

    expect(areaRows()).toHaveLength(1);
    expect(rowText(areaRows()[0])).toContain("写真");
    expect(callsOf(calls, "check_on_top")).toHaveLength(1);
  });

  it("draws no frame for an image whose photo candidate was put aside", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      detect_layout: () => candidates,
    });
    fireEvent.keyDown(candidateOf("photo"), { key: "Delete" });
    await flush();
    expect(
      screen.getAllByTestId("crop-candidate").map((c) => c.dataset.kind),
    ).toEqual(["chart"]);
    expect(screen.queryAllByTestId("crop-photo-frame")).toHaveLength(0);
  });

  it("words the frame of an image without a photo candidate like one, until an area lies in its place", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      detect_layout: () => [candidate("chart", [50, 60, 250, 160])],
    });
    const frame = screen.getByTestId("crop-photo-frame");
    expect(frame.textContent).toBe("写真");

    fireEvent.click(frame);
    await flush();
    expect(screen.getByTestId("crop-photo-frame").textContent).toBe("");
  });

  it("keeps the areas when the candidates are hidden", async () => {
    await openPdfInCropTab({ detect_layout: () => candidates });
    fireEvent.click(candidateOf("chart"));
    toggleCandidates();

    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);
    expect(screen.getAllByTestId("crop-area")).toHaveLength(1);
  });

  it("leaves the word to the area in its place, and takes it back once the area moves away", async () => {
    await openPdfInCropTab({
      list_page_images: () => photos,
      detect_layout: () => candidates,
    });
    const chart = candidateOf("chart");
    const photo = candidateOf("photo");
    fireEvent.click(chart);
    await flush();

    expect(chart).not.toHaveTextContent("グラフ");
    expect(chart).toHaveClass("is-covered");
    expect(photo).toHaveTextContent("写真");
    expect(photo).not.toHaveClass("is-covered");
    expect(screen.getByTestId("crop-area")).toHaveTextContent("グラフ");

    // 10 pt to the right still overlaps by 190 / 210 of the 200 pt width,
    // over 0.9; 20 pt leaves 180 / 220, under it.
    const moveRight = () =>
      fireEvent.keyDown(screen.getByTestId("crop-area"), {
        key: "ArrowRight",
        shiftKey: true,
      });
    moveRight();
    await flush();
    expect(chart).not.toHaveTextContent("グラフ");
    moveRight();
    await flush();
    expect(chart).toHaveTextContent("グラフ");
    expect(chart).not.toHaveClass("is-covered");
  });

  it("leaves the line and the word of a photo's frame to the area in its place", async () => {
    await openPdfInCropTab({ list_page_images: () => photos });
    const frame = screen.getByTestId("crop-photo-frame");
    expect(frame).not.toHaveClass("is-covered");
    fireEvent.click(frame);
    await flush();
    expect(screen.getByTestId("crop-photo-frame")).toHaveClass("is-covered");
  });

  it("draws an area whose row is unchecked as one that is not exported", async () => {
    await openPdfInCropTab({ detect_layout: () => candidates });
    fireEvent.click(candidateOf("chart"));
    await flush();
    const area = () => screen.getByTestId("crop-area");
    expect(area()).not.toHaveClass("is-unchecked");

    const check = screen.getByRole("checkbox", {
      name: "p.1 のグラフを書き出す",
    });
    fireEvent.click(check);
    expect(area()).toHaveClass("is-unchecked");
    fireEvent.click(check);
    expect(area()).not.toHaveClass("is-unchecked");
  });
});

describe("selecting", () => {
  it("selects an area that is pressed, and none when the page is pressed elsewhere", async () => {
    await openPdfInCropTab({
      detect_layout: () => [
        candidate("chart", [50, 60, 250, 160]),
        candidate("table", [50, 300, 250, 400]),
      ],
    });
    const [chart, table] = screen.getAllByTestId("crop-candidate");
    fireEvent.click(chart);
    fireEvent.click(table);
    expect(selectedAreas().map((a) => a.dataset.kind)).toEqual(["table"]);

    fireEvent.click(screen.getAllByTestId("crop-area")[0]);
    expect(selectedAreas().map((a) => a.dataset.kind)).toEqual(["chart"]);
    expect(
      screen.getByRole("button", { name: "p.1 のグラフを表示" }),
    ).toHaveAttribute("aria-current", "true");
    expect(
      screen.getByRole("button", { name: "p.1 の表を表示" }),
    ).not.toHaveAttribute("aria-current");

    const overlay = screen
      .getByTestId("crop-page")
      .querySelector(".crop-overlay");
    expect(overlay).not.toBeNull();
    if (overlay !== null) {
      fireEvent.click(overlay);
    }
    expect(selectedAreas()).toHaveLength(0);
    expect(screen.getAllByTestId("crop-area")).toHaveLength(2);
  });

  it("tells the selected area from the others by the ground and the word", async () => {
    await openPdfInCropTab({
      detect_layout: () => [candidate("chart", [50, 60, 250, 160])],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    expect(screen.getByTestId("crop-area")).toHaveClass("is-selected");
    fireEvent.click(
      screen.getByTestId("crop-area").parentElement ?? document.body,
    );
    expect(screen.getByTestId("crop-area")).not.toHaveClass("is-selected");
  });
});

describe("the menu of areas to add", () => {
  it("has the three items, in a menu button, with drawing set apart", async () => {
    await openPdfInCropTab();
    const button = screen.getByRole("button", { name: "範囲を追加" });
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getAllByRole("menuitem").map((item) => item.textContent),
    ).toEqual([
      "このページの候補をすべて追加",
      "全ページの候補をすべて追加",
      "範囲を描く",
    ]);
    const menu = screen.getByRole("menu");
    expect(
      [...menu.children].map((child) => child.getAttribute("role")),
    ).toEqual(["menuitem", "menuitem", "separator", "menuitem"]);
  });

  it("closes on Esc, with the focus back on the button, and on a press outside", async () => {
    await openPdfInCropTab();
    const button = screen.getByRole("button", { name: "範囲を追加" });
    fireEvent.click(button);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(button).toHaveFocus();

    fireEvent.click(button);
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("moves between the items with the arrow keys", async () => {
    await openPdfInCropTab({
      detect_layout: () => [candidate("chart", [50, 60, 250, 160])],
    });
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加" }));
    expect(
      screen.getByRole("menuitem", { name: "このページの候補をすべて追加" }),
    ).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
    expect(
      screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
    ).toHaveFocus();
  });

  describe("this page's candidates", () => {
    it("cannot be added when the page has none", async () => {
      await openPdfInCropTab();
      fireEvent.click(screen.getByRole("button", { name: "範囲を追加" }));
      expect(
        screen.getByRole("menuitem", { name: "このページの候補をすべて追加" }),
      ).toBeDisabled();
    });

    it("cannot be added while the page is being analyzed", async () => {
      await openPdfInCropTab({ detect_layout: () => new Promise(() => {}) });
      fireEvent.click(screen.getByRole("button", { name: "範囲を追加" }));
      expect(
        screen.getByRole("menuitem", { name: "このページの候補をすべて追加" }),
      ).toBeDisabled();
    });

    it("adds every candidate in order, selecting none, even with the candidates hidden", async () => {
      await openPdfInCropTab({
        detect_layout: () => [
          candidate("table", [50, 300, 250, 400]),
          candidate("chart", [50, 60, 250, 160]),
        ],
      });
      toggleCandidates();
      await addMenuItem("このページの候補をすべて追加");

      expect(areaRows().map((row) => rowText(row))).toEqual([
        expect.stringContaining("表"),
        expect.stringContaining("グラフ"),
      ]);
      expect(areaRows().map((row) => row.dataset.key)).toEqual(["1", "2"]);
      expect(selectedAreas()).toHaveLength(0);
    });

    it("skips a candidate that is already an area", async () => {
      await openPdfInCropTab({
        detect_layout: () => [
          candidate("table", [50, 300, 250, 400]),
          candidate("chart", [50, 60, 250, 160]),
        ],
      });
      fireEvent.click(screen.getAllByTestId("crop-candidate")[0]);
      await addMenuItem("このページの候補をすべて追加");
      expect(areaRows()).toHaveLength(2);
      await addMenuItem("このページの候補をすべて追加");
      expect(areaRows()).toHaveLength(2);
    });
  });

  describe("all pages' candidates", () => {
    it("starts the analysis of every page", async () => {
      const calls = await openPdfInCropTab();
      await addMenuItem("全ページの候補をすべて追加");
      expect(callsOf(calls, "start_detect_all").map((c) => c.args)).toEqual([
        { id: 7 },
      ]);
    });

    it("cannot be chosen when the analysis is unavailable", async () => {
      await openPdfInCropTab({
        detect_layout: () => {
          throw { code: "LayoutUnavailable", detail: null };
        },
      });
      fireEvent.click(screen.getByRole("button", { name: "範囲を追加" }));
      expect(
        screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
      ).toBeDisabled();
    });
  });
});

describe("the list of areas", () => {
  async function twoPages() {
    const calls = await openPdfInCropTab();
    pickPage(3);
    await flush();
    await addWholePage();
    pickPage(1);
    await flush();
    await addWholePage();
    return calls;
  }

  it("goes by page, and by key within a page", async () => {
    await openPdfInCropTab({
      detect_layout: () => [
        candidate("table", [50, 300, 250, 400]),
        candidate("chart", [50, 60, 250, 160]),
      ],
    });
    pickPage(3);
    await flush();
    await addWholePage();
    pickPage(1);
    await flush();
    await addMenuItem("このページの候補をすべて追加");

    expect(areaRows().map((row) => `${rowPage(row)}${rowText(row)}`)).toEqual([
      expect.stringMatching(/^p\.1表/),
      expect.stringMatching(/^p\.1グラフ/),
      expect.stringMatching(/^p\.3手で選んだ範囲/),
    ]);
    expect(areaRows().map((row) => row.dataset.key)).toEqual(["2", "3", "1"]);
  });

  it("brings the row of an area selected on the page into view", async () => {
    await openPdfInCropTab({
      detect_layout: () => [
        candidate("chart", [50, 60, 250, 160]),
        candidate("table", [50, 300, 250, 400]),
      ],
    });
    const scroll = vi.spyOn(Element.prototype, "scrollIntoView");
    const rowOf = (kind: string) =>
      areaRows().find((row) => rowText(row).startsWith(kind));
    const [chart, table] = screen.getAllByTestId("crop-candidate");

    fireEvent.click(chart);
    expect(scroll.mock.contexts).toEqual([rowOf("グラフ")]);
    expect(scroll).toHaveBeenLastCalledWith({ block: "nearest" });

    fireEvent.click(table);
    expect(scroll.mock.contexts.at(-1)).toBe(rowOf("表"));

    fireEvent.click(screen.getAllByTestId("crop-area")[0]);
    expect(scroll.mock.contexts.at(-1)).toBe(rowOf("グラフ"));
    expect(scroll).toHaveBeenCalledTimes(3);
  });

  it("puts the rows of each page under a heading with the page and its count", async () => {
    await openPdfInCropTab({
      detect_layout: () => [
        candidate("table", [50, 300, 250, 400]),
        candidate("chart", [50, 60, 250, 160]),
      ],
    });
    pickPage(3);
    await flush();
    await addWholePage();
    pickPage(1);
    await flush();
    await addMenuItem("このページの候補をすべて追加");

    expect(pageHeads()).toEqual(["p.1 · 2 個", "p.3 · 1 個"]);
    const page1 = screen.getByRole("list", { name: "p.1 · 2 個" });
    expect(within(page1).getAllByTestId("crop-area-row")).toHaveLength(2);
    const page3 = screen.getByRole("list", { name: "p.3 · 1 個" });
    expect(rowText(within(page3).getByTestId("crop-area-row"))).toContain(
      "手で選んだ範囲",
    );
    expect(areaRows().every((row) => !rowText(row).includes("p."))).toBe(true);
  });

  it("says that there are no areas while there are none", async () => {
    await openPdfInCropTab();
    expect(screen.getByText("範囲はまだありません")).toBeInTheDocument();
    expect(
      screen.getByText(
        "ページの候補や写真を押すと、範囲になります。候補にないものは「範囲を描く」で描けます。",
      ),
    ).toBeInTheDocument();
  });

  it("starts the list without a heading of its own", async () => {
    await twoPages();
    const side = screen.getByRole("region", { name: "範囲と書き出しの設定" });
    expect(within(side).queryByText("範囲")).toBeNull();
    expect(pageHeads()[0]).toMatch(/^p\.1 · /);
  });

  it("leaves the counts to the bottom bar, not the list", async () => {
    await twoPages();
    const side = screen.getByRole("region", { name: "範囲と書き出しの設定" });
    expect(within(side).queryByText(/書き出す \d+ 個/)).toBeNull();
    expect(
      screen.getByText("2 個の範囲を PNG で書き出します"),
    ).toBeInTheDocument();
  });

  it("changes what the bottom bar says when a box is unchecked", async () => {
    await twoPages();
    expect(
      screen.getByText("2 個の範囲を PNG で書き出します"),
    ).toBeInTheDocument();

    const boxes = screen.getAllByRole("checkbox", { name: /を書き出す$/ });
    fireEvent.click(boxes[0]);
    expect(
      screen.getByText("1 個の範囲を PNG で書き出します"),
    ).toBeInTheDocument();
    fireEvent.click(boxes[1]);
    expect(screen.getByText("範囲が選ばれていません")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "書き出す" })).toBeDisabled();
  });

  it("names the format the bottom bar will write", async () => {
    await openPdfInCropTab();
    await addWholePage();
    openSettings();
    fireEvent.click(screen.getByRole("button", { name: "JPEG" }));
    expect(
      screen.getByText("1 個の範囲を JPEG で書き出します"),
    ).toBeInTheDocument();
  });

  it("shows the page of a row that is pressed and selects its area", async () => {
    await twoPages();
    pickPage(2);
    await flush();
    expectPageShown(2, 4);

    fireEvent.click(
      screen.getByRole("button", { name: "p.3 の手で選んだ範囲を表示" }),
    );
    await flush();

    expectPageShown(3, 4);
    expect(selectedAreas().map((a) => a.dataset.key)).toEqual(["1"]);
    expect(
      screen.getByRole("button", { name: "p.3 の手で選んだ範囲を表示" }),
    ).toHaveAttribute("aria-current", "true");
  });

  it("removes an area with its button, and selects none if it was selected", async () => {
    await twoPages();
    expect(selectedAreas().map((a) => a.dataset.key)).toEqual(["2"]);
    // Page 1 is shown: its area is the selected one.
    fireEvent.click(
      screen.getByRole("button", { name: "p.1 の手で選んだ範囲を消す" }),
    );
    expect(areaRows()).toHaveLength(1);
    expect(selectedAreas()).toHaveLength(0);
    expect(screen.queryByTestId("crop-area")).toBeNull();
  });

  it("keeps the selection when another area is removed", async () => {
    await twoPages();
    fireEvent.click(
      screen.getByRole("button", { name: "p.3 の手で選んだ範囲を消す" }),
    );
    expect(selectedAreas().map((a) => a.dataset.key)).toEqual(["2"]);
  });

  it("asks for the page's picture at the size the preview needs and releases it with the row", async () => {
    const calls = await openPdfInCropTab();
    await addWholePage();
    const previews = callsOf(calls, "get_thumbnail").filter(
      (c) => c.args.maxSide === 400,
    );
    // The page itself (fit to a 600 × 800 room) and the preview both fit
    // in the smallest step.
    expect(previews.length).toBeGreaterThanOrEqual(1);
    const row = areaRows()[0];
    const image = row.querySelector("img");
    expect(image).not.toBeNull();
    const src = image?.getAttribute("src") ?? "";
    expect(src).toMatch(/^blob:/);

    fireEvent.click(
      screen.getByRole("button", { name: "p.1 の手で選んだ範囲を消す" }),
    );
    await flush();
    expect(urls.revoke).toHaveBeenCalledWith(src);
  });

  it("shows only the area in the preview box, whole and centered", async () => {
    await openPdfInCropTab({
      detect_layout: () => [candidate("table", [0, 0, 595, 100])],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    const image = areaRows()[0].querySelector("img");
    expect(image).not.toBeNull();
    const area = image?.parentElement;
    // 595 × 100 pt in a 44 × 34 box: 44 / 595 px per point, 7.4 px tall.
    expect(parseFloat(area?.style.width ?? "")).toBeCloseTo(44);
    expect(parseFloat(area?.style.height ?? "")).toBeCloseTo((100 * 44) / 595);
    expect(parseFloat(area?.style.top ?? "")).toBeCloseTo(
      (34 - (100 * 44) / 595) / 2,
    );
    expect(parseFloat(image?.style.width ?? "")).toBeCloseTo(44);
    expect(parseFloat(image?.style.left ?? "")).toBeCloseTo(0);
    expect(parseFloat(image?.style.top ?? "")).toBeCloseTo(0);
  });

  it("says nothing of the size while a photo's page has no images", async () => {
    await openPdfInCropTab({
      list_page_images: () => new Promise(() => {}),
      detect_layout: () => [
        { ...candidate("photo", PHOTO_RECT), imageIndex: 2 },
      ],
    });
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await flush();
    const [row] = areaRows();
    expect(rowText(row)).toContain("写真");
    expect(rowText(row)).not.toMatch(/dpi|元の画素/);
  });

  it("sizes a photo by the resolution when original pixels are off", async () => {
    await openPdfInCropTab({ list_page_images: () => photos });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    await flush();
    expect(rowText(areaRows()[0])).toContain("元の画素 100 × 80");

    openSettings();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "写真は元の画質で書き出す" }),
    );
    expect(rowText(areaRows()[0])).toContain("150 dpi · 417 × 417");
  });

  it("sizes the areas by the resolution of the settings", async () => {
    await openPdfInCropTab();
    await addWholePage();
    openSettings();
    fireEvent.change(screen.getByLabelText("解像度"), {
      target: { value: "300" },
    });
    expect(rowText(areaRows()[0])).toContain("300 dpi · 2479 × 3508");
  });
});

describe("the page thumbnails", () => {
  it("count the areas of their page, the unchecked ones too, and say so in their names", async () => {
    await openPdfInCropTab({
      detect_layout: () => [
        candidate("table", [50, 300, 250, 400]),
        candidate("chart", [50, 60, 250, 160]),
      ],
    });
    await addMenuItem("このページの候補をすべて追加");
    fireEvent.click(
      screen.getAllByRole("checkbox", { name: /を書き出す$/ })[0],
    );

    const first = screen.getByRole("button", { name: "ページ 1、範囲 2 個" });
    expect(first).toHaveTextContent("2");
    expect(within(first).getByText("2")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "ページ 2" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /ページ 2、/ })).toBeNull();
  });

  it("follow the areas that are added and removed", async () => {
    await openPdfInCropTab();
    await addWholePage();
    expect(
      screen.getByRole("button", { name: "ページ 1、範囲 1 個" }),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "p.1 の手で選んだ範囲を消す" }),
    );
    expect(
      screen.getByRole("button", { name: "ページ 1" }),
    ).toBeInTheDocument();
  });
});

describe("in English", () => {
  it("says the counts in the singular and the plural", async () => {
    await openPdfInCropTab({}, cropPdf(), "en");
    fireEvent.click(screen.getByRole("button", { name: "Add areas" }));
    expect(
      screen.getAllByRole("menuitem").map((item) => item.textContent),
    ).toEqual([
      "Add all suggestions on this page",
      "Add all suggestions on all pages",
      "Draw an area",
    ]);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    await drawWholePage();

    expect(screen.getByText("Export 1 area as PNG")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Page 1, 1 area" }),
    ).toBeInTheDocument();
    expect(rowText(areaRows()[0])).toContain("Custom area");
    expect(
      screen.getByRole("button", { name: "Show Custom area on p.1" }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Page 2" }));
    await flush();
    await drawWholePage();
    expect(screen.getByText("Export 2 areas as PNG")).toBeInTheDocument();
  });

  it("heads each page with its count in the singular and the plural", async () => {
    await openPdfInCropTab(
      {
        detect_layout: () => [
          candidate("table", [50, 300, 250, 400]),
          candidate("chart", [50, 60, 250, 160]),
        ],
      },
      cropPdf(),
      "en",
    );
    await addMenuItem("Add all suggestions on this page", "Add areas");
    fireEvent.click(screen.getByRole("button", { name: "Page 2" }));
    await flush();
    await drawWholePage();
    expect(pageHeads()).toEqual(["p.1 · 2 areas", "p.2 · 1 area"]);
  });
});
