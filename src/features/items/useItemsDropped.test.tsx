import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  ITEMS_DROPPED_EVENT,
  type ImageItem,
  type ItemsDropped,
  type PdfItem,
} from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  useAppDispatch,
  useAppState,
} from "../../state";
import { tabAfterDrop, useItemsDropped } from "./useItemsDropped";

const image: ImageItem = {
  id: 1,
  name: "a.png",
  width: 10,
  height: 10,
  format: "png",
  bytes: 100,
  error: null,
};

const pdf: PdfItem = {
  id: 2,
  name: "b.pdf",
  pageCount: 3,
  firstPageSizePt: { widthPt: 595, heightPt: 842 },
  bytes: 200,
  error: null,
};

function dropped(parts: Partial<ItemsDropped>): ItemsDropped {
  return {
    images: [],
    pdfs: [],
    skipped: { unsupported: 0, folders: 0, duplicates: 0 },
    error: null,
    ...parts,
  };
}

describe("tabAfterDrop", () => {
  it("goes to the list that received items", () => {
    expect(tabAfterDrop(dropped({ images: [image] }), "pdfToImages")).toBe(
      "imagesToPdf",
    );
    expect(tabAfterDrop(dropped({ pdfs: [pdf] }), "imagesToPdf")).toBe(
      "pdfToImages",
    );
  });

  it("stays when both lists or neither received items", () => {
    expect(
      tabAfterDrop(dropped({ images: [image], pdfs: [pdf] }), "pdfToImages"),
    ).toBe("pdfToImages");
    expect(tabAfterDrop(dropped({}), "imagesToPdf")).toBe("imagesToPdf");
  });
});

function Harness() {
  const { error, dismiss } = useItemsDropped();
  const { language, imagesToPdf, pdfToImages } = useAppState();
  const dispatch = useAppDispatch();
  return (
    <>
      <output data-testid="tab">{language.activeTab}</output>
      <output data-testid="images">
        {imagesToPdf.items.map((item) => item.name).join(",")}
      </output>
      <output data-testid="pdfs">
        {pdfToImages.items.map((item) => item.name).join(",")}
      </output>
      <output data-testid="error">{error?.code ?? ""}</output>
      <button type="button" onClick={dismiss}>
        dismiss
      </button>
      <button
        type="button"
        onClick={() => dispatch({ type: "SET_ACTIVE_TAB", tab: "pdfToImages" })}
      >
        to pdfs
      </button>
    </>
  );
}

async function drop(payload: ItemsDropped) {
  await act(() => emit(ITEMS_DROPPED_EVENT, payload));
}

describe("useItemsDropped", () => {
  beforeEach(() => {
    mockIPC(() => undefined, { shouldMockEvents: true });
    render(
      <AppStateProvider initialState={createInitialAppState("ja-JP")}>
        <Harness />
      </AppStateProvider>,
    );
  });

  // Unmount first: unmounting unlistens, which needs the mocked events.
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("adds dropped PDFs and switches to their tab", async () => {
    await drop(dropped({ pdfs: [pdf] }));

    expect(screen.getByTestId("pdfs")).toHaveTextContent("b.pdf");
    expect(screen.getByTestId("tab")).toHaveTextContent("pdfToImages");
  });

  it("fills both lists from one drop and keeps the tab", async () => {
    await drop(dropped({ images: [image], pdfs: [pdf] }));

    expect(screen.getByTestId("images")).toHaveTextContent("a.png");
    expect(screen.getByTestId("pdfs")).toHaveTextContent("b.pdf");
    expect(screen.getByTestId("tab")).toHaveTextContent("imagesToPdf");
  });

  it("decides the tab from the one shown when the drop arrives", async () => {
    fireEvent.click(screen.getByRole("button", { name: "to pdfs" }));
    await drop(dropped({ images: [image], pdfs: [pdf] }));

    expect(screen.getByTestId("tab")).toHaveTextContent("pdfToImages");
  });

  it("switches to images from the PDF tab the user moved to", async () => {
    fireEvent.click(screen.getByRole("button", { name: "to pdfs" }));
    await drop(dropped({ images: [image] }));

    expect(screen.getByTestId("tab")).toHaveTextContent("imagesToPdf");
  });

  it("keeps every drop when two arrive back to back", async () => {
    await act(async () => {
      await emit(ITEMS_DROPPED_EVENT, dropped({ images: [image] }));
      await emit(
        ITEMS_DROPPED_EVENT,
        dropped({ images: [{ ...image, id: 3, name: "c.png" }] }),
      );
    });

    expect(screen.getByTestId("images")).toHaveTextContent("a.png,c.png");
  });

  it("reports a refused drop until it is dismissed", async () => {
    await drop(dropped({ error: { code: "ConversionRunning", detail: null } }));
    expect(screen.getByTestId("error")).toHaveTextContent("ConversionRunning");

    fireEvent.click(screen.getByRole("button", { name: "dismiss" }));
    expect(screen.getByTestId("error")).toHaveTextContent("");
  });

  it("clears the error with the next drop", async () => {
    await drop(dropped({ error: { code: "ConversionRunning", detail: null } }));
    await drop(dropped({ images: [image] }));

    expect(screen.getByTestId("error")).toHaveTextContent("");
  });
});
