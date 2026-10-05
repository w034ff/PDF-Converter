import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useThumbnail } from "./useThumbnail";

const PNG_BYTES = [137, 80, 78, 71];

interface Call {
  cmd: string;
  args: unknown;
}

function mockThumbnails(answer: () => unknown = () => PNG_BYTES): Call[] {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    return answer();
  });
  return calls;
}

function Probe({ id, page }: { id: number; page?: number }) {
  const { ref, src, failed } = useThumbnail(id, page);
  return (
    <div ref={ref}>
      <output data-testid="src">{src ?? ""}</output>
      <output data-testid="failed">{String(failed)}</output>
    </div>
  );
}

interface FakeObserver {
  callback: (entries: IntersectionObserverEntry[]) => void;
  observed: Element[];
}

const observers: FakeObserver[] = [];

/** Stands in for the browser's observer, which jsdom does not have. */
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

/** Reports every element `fake` observes as on screen. */
function show(fake: FakeObserver) {
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

describe("useThumbnail", () => {
  let nextUrl = 0;
  const createObjectURL = vi.fn<(blob: Blob) => string>(
    () => `blob:thumb-${++nextUrl}`,
  );
  const revokeObjectURL = vi.fn();

  beforeEach(() => {
    vi.stubGlobal("URL", { ...URL, createObjectURL, revokeObjectURL });
    nextUrl = 0;
    createObjectURL.mockClear();
    revokeObjectURL.mockClear();
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    vi.unstubAllGlobals();
    observers.length = 0;
  });

  it("asks for the item and page and shows the picture as a blob URL", async () => {
    const calls = mockThumbnails();
    render(<Probe id={4} page={2} />);

    await waitFor(() =>
      expect(screen.getByTestId("src")).toHaveTextContent("blob:thumb-"),
    );
    expect(calls).toEqual([{ cmd: "get_thumbnail", args: { id: 4, page: 2 } }]);
    const [blob] = createObjectURL.mock.calls[0];
    expect(blob.type).toBe("image/png");
  });

  it("releases the blob URL when it unmounts", async () => {
    mockThumbnails();
    const { unmount } = render(<Probe id={4} />);
    await waitFor(() =>
      expect(screen.getByTestId("src")).toHaveTextContent("blob:thumb-"),
    );
    const url = screen.getByTestId("src").textContent;

    unmount();

    expect(revokeObjectURL).toHaveBeenCalledWith(url);
  });

  it("drops the old picture and loads the new one when the item changes", async () => {
    const calls = mockThumbnails();
    const { rerender } = render(<Probe id={4} />);
    await waitFor(() =>
      expect(screen.getByTestId("src")).toHaveTextContent("blob:thumb-1"),
    );

    rerender(<Probe id={5} />);

    await waitFor(() =>
      expect(screen.getByTestId("src")).toHaveTextContent("blob:thumb-2"),
    );
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:thumb-1");
    expect(calls.map((call) => call.args)).toEqual([
      { id: 4, page: undefined },
      { id: 5, page: undefined },
    ]);
  });

  it("shows nothing for the new item until its picture arrives", async () => {
    let requests = 0;
    mockThumbnails(() => {
      requests += 1;
      return requests === 1 ? PNG_BYTES : new Promise(() => {});
    });
    const { rerender } = render(<Probe id={4} />);
    await waitFor(() =>
      expect(screen.getByTestId("src")).toHaveTextContent("blob:thumb-1"),
    );

    rerender(<Probe id={5} />);

    expect(screen.getByTestId("src")).toHaveTextContent("");
    await waitFor(() => expect(requests).toBe(2));
    expect(screen.getByTestId("src")).toHaveTextContent("");
  });

  it("reports a failed request", async () => {
    mockThumbnails(() => {
      throw { code: "ReadFailed", detail: null };
    });
    render(<Probe id={4} />);

    await waitFor(() =>
      expect(screen.getByTestId("failed")).toHaveTextContent("true"),
    );
    expect(screen.getByTestId("src")).toHaveTextContent("");
  });

  it("waits until the element comes into view", async () => {
    vi.stubGlobal("IntersectionObserver", FakeIntersectionObserver);
    const calls = mockThumbnails();
    render(<Probe id={4} />);

    await waitFor(() => expect(observers).toHaveLength(1));
    expect(calls).toEqual([]);

    show(observers[0]);

    await waitFor(() =>
      expect(screen.getByTestId("src")).toHaveTextContent("blob:thumb-"),
    );
    expect(calls).toHaveLength(1);
  });
});
