import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import type { CropRegionInput, OutputDirLabel, Settings } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  useAppState,
} from "../../state";
import { DEFAULT_SETTINGS } from "../crop/cropTestSupport";
import { JobFooter } from "./JobFooter";
import { useJobEvents } from "./useJobEvents";
import { useJobRunner } from "./useJobRunner";

const PDFS_DIR: OutputDirLabel = { dirLabel: "pdfs" };
const IMAGES_DIR: OutputDirLabel = { dirLabel: "images" };
const CROP_DIR: OutputDirLabel = { dirLabel: "crops" };

const SETTINGS: Settings = {
  ...DEFAULT_SETTINGS,
  imagesToPdf: { ...DEFAULT_SETTINGS.imagesToPdf, outputDir: PDFS_DIR },
  pdfToImages: { ...DEFAULT_SETTINGS.pdfToImages, outputDir: IMAGES_DIR },
  crop: { ...DEFAULT_SETTINGS.crop, outputDir: CROP_DIR },
};

const REGIONS: CropRegionInput[] = [
  { key: 4, page: 1, rectPt: [0, 0, 10, 10], imageIndex: null },
  { key: 2, page: 3, rectPt: [5, 5, 50, 50], imageIndex: 6 },
];

const ORGANIZE_PAGES = [
  { id: 3, page: 2, rotation: 90 },
  { id: 3, page: 1, rotation: 0 },
];

function Harness() {
  const [replaced, setReplaced] = useState<number | null | undefined>(
    undefined,
  );
  useJobEvents();
  const runner = useJobRunner();
  const { job, crop, imagesToPdf, pdfToImages } = useAppState();
  return (
    <>
      <button
        type="button"
        onClick={() =>
          void runner.startCropExport(
            7,
            [
              { id: 4, name: "doc.pdf" },
              { id: 2, name: "doc.pdf" },
            ],
            REGIONS,
            "webp",
            "gray",
            300,
            false,
            true,
          )
        }
      >
        export
      </button>
      <button type="button" onClick={() => void runner.startDetectAll(7)}>
        detect all
      </button>
      <button
        type="button"
        onClick={() =>
          void runner
            .saveOrganizedPdf(ORGANIZE_PAGES, null)
            .then((replacedId) => setReplaced(replacedId))
        }
      >
        save organized
      </button>
      <output data-testid="replaced">{String(replaced)}</output>
      <output data-testid="job">
        {JSON.stringify({
          phase: job.phase,
          kind: job.kind,
          targets: job.targets,
          lastJob: crop.lastJob,
          error: job.error?.code ?? null,
        })}
      </output>
      <output data-testid="dirs">
        {JSON.stringify({
          crop: crop.outputDir?.dirLabel ?? null,
          pdfToImages: pdfToImages.outputDir?.dirLabel ?? null,
          imagesToPdf: imagesToPdf.outputDir?.dirLabel ?? null,
        })}
      </output>
      <JobFooter tab="crop" idleStatus={<span>idle</span>} action={null} />
    </>
  );
}

function renderHarness() {
  return render(
    <AppStateProvider initialState={createInitialAppState("ja-JP", SETTINGS)}>
      <Harness />
    </AppStateProvider>,
  );
}

function readJson(testId: string): unknown {
  return JSON.parse(screen.getByTestId(testId).textContent ?? "null");
}

interface Call {
  cmd: string;
  args: unknown;
}

function mockCommands(answer: (cmd: string) => unknown): Call[] {
  const calls: Call[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      return answer(cmd);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

describe("useJobRunner for the crop tab", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("starts a write of the areas as a crop conversion, under their keys", async () => {
    const calls = mockCommands(() => undefined);
    renderHarness();
    fireEvent.click(screen.getByRole("button", { name: "export" }));

    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "start_crop_export",
        args: {
          id: 7,
          regions: REGIONS,
          format: "webp",
          colorMode: "gray",
          dpi: 300,
          photoNative: false,
          photoOnly: true,
        },
      }),
    );
    expect(readJson("job")).toEqual({
      phase: "running",
      kind: "crop",
      targets: [
        { id: 4, name: "doc.pdf" },
        { id: 2, name: "doc.pdf" },
      ],
      lastJob: "export",
      error: null,
    });
  });

  it("starts the search of all pages as a crop conversion with no items", async () => {
    const calls = mockCommands(() => undefined);
    renderHarness();
    fireEvent.click(screen.getByRole("button", { name: "detect all" }));

    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "start_detect_all",
        args: { id: 7 },
      }),
    );
    expect(readJson("job")).toEqual({
      phase: "running",
      kind: "crop",
      targets: [],
      lastJob: "detectAll",
      error: null,
    });
    expect(
      screen.getByText("全ページの候補を探しています…"),
    ).toBeInTheDocument();
  });

  it("forgets the crop folder only when Rust has forgotten it", async () => {
    mockCommands((cmd) => {
      if (cmd === "start_crop_export") {
        throw { code: "OutputDirMissing", detail: null };
      }
      return undefined;
    });
    renderHarness();
    expect(readJson("dirs")).toEqual({
      crop: "crops",
      pdfToImages: "images",
      imagesToPdf: "pdfs",
    });

    fireEvent.click(screen.getByRole("button", { name: "export" }));

    await waitFor(() =>
      expect(readJson("dirs")).toEqual({
        crop: null,
        pdfToImages: "images",
        imagesToPdf: "pdfs",
      }),
    );
    expect(readJson("job")).toMatchObject({ error: "OutputDirMissing" });
  });

  it("keeps the folders when a start is refused for another reason", async () => {
    mockCommands((cmd) => {
      if (cmd === "start_detect_all") {
        throw { code: "ConversionRunning", detail: null };
      }
      return undefined;
    });
    renderHarness();
    fireEvent.click(screen.getByRole("button", { name: "detect all" }));
    await waitFor(() =>
      expect(readJson("job")).toMatchObject({ error: "ConversionRunning" }),
    );
    expect(readJson("dirs")).toEqual({
      crop: "crops",
      pdfToImages: "images",
      imagesToPdf: "pdfs",
    });
  });
});

describe("useJobRunner for the organize tab", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("starts the save as an organize conversion with no items, since its outputs are not items", async () => {
    let answer: (value: unknown) => void = () => {};
    const calls = mockCommands(
      () => new Promise((resolve) => (answer = resolve)),
    );
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save organized" }));

    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "save_organized_pdf",
        args: { pages: ORGANIZE_PAGES, password: null },
      }),
    );
    expect(readJson("job")).toMatchObject({
      phase: "running",
      kind: "organize",
      targets: [],
    });
    answer({ savedName: "a.pdf", replacedId: null });
    await waitFor(() =>
      expect(screen.getByTestId("replaced")).toHaveTextContent("null"),
    );
  });

  it("gives back the ID of the PDF the save replaced", async () => {
    mockCommands(() => ({ savedName: "a.pdf", replacedId: 3 }));
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save organized" }));

    await waitFor(() =>
      expect(screen.getByTestId("replaced")).toHaveTextContent("3"),
    );
  });

  it("goes back to nothing when the save dialog is cancelled", async () => {
    mockCommands(() => null);
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save organized" }));

    await waitFor(() =>
      expect(readJson("job")).toMatchObject({ phase: "idle", kind: null }),
    );
    expect(screen.getByTestId("replaced")).toHaveTextContent("null");
  });

  it("reports a refusal as a failure of the organize tab and leaves every folder", async () => {
    mockCommands(() => {
      throw { code: "OutputTooLarge", detail: "613 MB / 512 MB" };
    });
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save organized" }));

    await waitFor(() =>
      expect(readJson("job")).toMatchObject({
        kind: "organize",
        error: "OutputTooLarge",
      }),
    );
    expect(readJson("dirs")).toEqual({
      crop: "crops",
      pdfToImages: "images",
      imagesToPdf: "pdfs",
    });
  });

  it("does not take a missing folder for the folder of another tab", async () => {
    mockCommands(() => {
      throw { code: "OutputDirMissing", detail: null };
    });
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save organized" }));

    await waitFor(() =>
      expect(readJson("job")).toMatchObject({ error: "OutputDirMissing" }),
    );
    expect(readJson("dirs")).toEqual({
      crop: "crops",
      pdfToImages: "images",
      imagesToPdf: "pdfs",
    });
  });
});
