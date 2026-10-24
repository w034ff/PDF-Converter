import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  areaRows,
  callsOf,
  cropPdf,
  deferred,
  dragFromTo,
  drawFromTo,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  mockCropIpc,
  openPdfInCropTab,
  overlay,
  PNG_BYTES,
  type ObjectUrls,
} from "./cropTestSupport";
import { useAreaPreview } from "./useAreaPreview";

let urls: ObjectUrls;

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  urls = installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

/** The previews are asked for at this size for an area 100 pt long, and at the next for one 30 pt long. */
const SIZE_OF_100_PT = 400;
const SIZE_OF_30_PT = 1600;

function previewCalls(calls: ReturnType<typeof mockCropIpc>, maxSide: number) {
  return callsOf(calls, "get_thumbnail").filter(
    (call) => call.args.maxSide === maxSide && call.args.page === 1,
  );
}

function rowImage(index: number): HTMLElement | null {
  return areaRows()[index].querySelector<HTMLElement>("img");
}

function drawSquare(x: number, y = 500, side = 100) {
  drawFromTo(overlay(), [x, y], [x + side, y + side]);
}

describe("the previews of the list of areas", () => {
  it("ask once for rows of the same page and size, and share the picture", async () => {
    const calls = await openPdfInCropTab();
    drawSquare(20);
    drawSquare(200);
    await flush();

    expect(areaRows()).toHaveLength(2);
    expect(previewCalls(calls, SIZE_OF_100_PT)).toHaveLength(1);
    const first = rowImage(0)?.getAttribute("src");
    expect(first).toMatch(/^blob:/);
    expect(rowImage(1)?.getAttribute("src")).toBe(first);
  });

  it("keep the picture while a row still uses it, and let go of it with the last", async () => {
    await openPdfInCropTab();
    drawSquare(20);
    drawSquare(200);
    await flush();
    const src = rowImage(0)?.getAttribute("src") ?? "";

    fireEvent.click(
      screen.getAllByRole("button", { name: "p.1 の手で選んだ範囲を消す" })[0],
    );
    await flush();
    expect(urls.revoke).not.toHaveBeenCalledWith(src);

    fireEvent.click(
      screen.getByRole("button", { name: "p.1 の手で選んだ範囲を消す" }),
    );
    await flush();
    expect(urls.revoke).toHaveBeenCalledWith(src);
  });

  it("let go of the pictures of the PDF that is replaced", async () => {
    let opened = 0;
    await openPdfInCropTab({
      open_crop_pdf: () => {
        opened += 1;
        return opened === 1 ? cropPdf() : cropPdf({ id: 8, name: "next.pdf" });
      },
    });
    drawSquare(20);
    await flush();
    const src = rowImage(0)?.getAttribute("src") ?? "";
    expect(src).toMatch(/^blob:/);

    fireEvent.click(screen.getByRole("button", { name: "別の PDF を開く" }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "破棄して開く" }));
    await flush();

    expect(screen.getByText("next.pdf")).toBeInTheDocument();
    expect(areaRows()).toHaveLength(0);
    expect(urls.revoke).toHaveBeenCalledWith(src);
  });

  it("show the picture they had until the one of a new size arrives", async () => {
    const big = deferred<number[]>();
    const calls = await openPdfInCropTab({
      get_thumbnail: (args) =>
        args.maxSide === SIZE_OF_30_PT ? big.promise : PNG_BYTES,
    });
    drawSquare(20);
    await flush();
    const before = rowImage(0)?.getAttribute("src") ?? "";
    expect(before).toMatch(/^blob:/);

    // Narrow the area from 100 pt to 30 pt: a bigger picture of the page is needed.
    const handle = screen
      .getAllByTestId("crop-handle")
      .find((element) => element.dataset.handle === "se");
    if (handle === undefined) {
      throw new Error("no handle");
    }
    dragFromTo(handle, [120, 600], [50, 530]);
    await flush();
    expect(previewCalls(calls, SIZE_OF_30_PT)).toHaveLength(1);
    expect(rowImage(0)?.getAttribute("src")).toBe(before);
    expect(urls.revoke).not.toHaveBeenCalledWith(before);

    await act(async () => big.resolve(PNG_BYTES));
    await flush();
    const after = rowImage(0)?.getAttribute("src") ?? "";
    expect(after).toMatch(/^blob:/);
    expect(after).not.toBe(before);
    expect(urls.revoke).toHaveBeenCalledWith(before);
  });

  it("show no picture for a size that could not be had, and ask for it once", async () => {
    const calls = await openPdfInCropTab({
      get_thumbnail: (args) => {
        if (args.maxSide === SIZE_OF_100_PT) {
          throw { code: "WorkerCrashed", detail: null };
        }
        return PNG_BYTES;
      },
    });
    drawSquare(20);
    drawSquare(200);
    await flush();
    await flush();
    expect(rowImage(0)).toBeNull();
    expect(rowImage(1)).toBeNull();
    expect(previewCalls(calls, SIZE_OF_100_PT)).toHaveLength(1);
  });
});

interface FakeObserver {
  callback: (entries: IntersectionObserverEntry[]) => void;
  observed: Element[];
}

const observers: FakeObserver[] = [];

function FakeIntersectionObserver(
  callback: (entries: IntersectionObserverEntry[]) => void,
) {
  const fake: FakeObserver = { callback, observed: [] };
  observers.push(fake);
  return {
    observe: (element: Element) => {
      fake.observed.push(element);
    },
    disconnect: () => {
      fake.observed = [];
    },
  };
}

function showObserved() {
  act(() => {
    for (const fake of observers) {
      fake.callback(
        fake.observed.map((target) => {
          const rect = target.getBoundingClientRect();
          return {
            target,
            isIntersecting: true,
            intersectionRatio: 1,
            boundingClientRect: rect,
            intersectionRect: rect,
            rootBounds: null,
            time: 0,
          };
        }),
      );
    }
  });
}

function Probe({ maxSide }: { maxSide: number }) {
  const { ref, src } = useAreaPreview(7, 1, maxSide);
  return (
    <div ref={ref}>
      <output data-testid="src">{src ?? ""}</output>
    </div>
  );
}

describe("useAreaPreview", () => {
  beforeEach(() => {
    observers.length = 0;
    vi.stubGlobal("IntersectionObserver", FakeIntersectionObserver);
  });

  it("asks only once the row has been seen, and then for every size without being seen again", async () => {
    const calls = mockCropIpc();
    const { rerender } = render(<Probe maxSide={400} />);
    await flush();
    expect(callsOf(calls, "get_thumbnail")).toHaveLength(0);

    showObserved();
    await flush();
    expect(callsOf(calls, "get_thumbnail").map((c) => c.args.maxSide)).toEqual([
      400,
    ]);
    expect(screen.getByTestId("src").textContent).toMatch(/^blob:/);

    rerender(<Probe maxSide={800} />);
    await flush();
    expect(callsOf(calls, "get_thumbnail").map((c) => c.args.maxSide)).toEqual([
      400, 800,
    ]);
  });
});

describe("useAreaPreview across PDFs", () => {
  it("shows nothing for another PDF's page until its own picture is there", async () => {
    const gate = deferred<number[]>();
    mockCropIpc({
      get_thumbnail: (args) => (args.id === 8 ? gate.promise : PNG_BYTES),
    });
    function Both({ id }: { id: number }) {
      const { ref, src } = useAreaPreview(id, 1, 400);
      return (
        <div ref={ref}>
          <output data-testid="src">{src ?? ""}</output>
        </div>
      );
    }
    const { rerender } = render(<Both id={7} />);
    await flush();
    expect(screen.getByTestId("src").textContent).toMatch(/^blob:/);

    rerender(<Both id={8} />);
    await flush();
    expect(screen.getByTestId("src").textContent).toBe("");
  });
});
