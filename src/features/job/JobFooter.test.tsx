import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  JOB_FINISHED_EVENT,
  JOB_ITEM_EVENT,
  JOB_PROGRESS_EVENT,
  type JobFinishedPayload,
  type JobProgressPayload,
} from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  useAppState,
  type ActiveTab,
  type JobTarget,
} from "../../state";
import { JobFooter } from "./JobFooter";
import { useJobEvents } from "./useJobEvents";
import { useJobRunner } from "./useJobRunner";

const targets: JobTarget[] = [
  { id: 1, name: "a.png" },
  { id: 2, name: "b.png" },
  { id: 3, name: "c.png" },
];

const finished: JobFinishedPayload = {
  succeeded: 3,
  failed: 0,
  noPages: 0,
  unprocessed: 0,
  cancelled: false,
};

interface Call {
  cmd: string;
  args: unknown;
}

/** Records every command and answers with `answer(cmd)`. */
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

function Harness({ tab = "imagesToPdf" }: { tab?: ActiveTab }) {
  useJobEvents();
  const runner = useJobRunner();
  const { job } = useAppState();
  return (
    <>
      <button
        type="button"
        onClick={() => void runner.startImagesToPdfs(targets, "fit")}
      >
        start each
      </button>
      <button
        type="button"
        onClick={() => void runner.saveMergedPdf(targets, "a4")}
      >
        save merged
      </button>
      <button
        type="button"
        onClick={() =>
          void runner.startPdfsToImages(
            [{ id: 7, name: "doc.pdf" }],
            "1-2",
            "jpeg",
            300,
          )
        }
      >
        start pdfs
      </button>
      <output data-testid="saved-name">{job.savedName ?? ""}</output>
      <JobFooter
        tab={tab}
        idleStatus={<span>idle</span>}
        action={<button type="button">screen action</button>}
      />
    </>
  );
}

function renderHarness(tab?: ActiveTab) {
  return render(
    <AppStateProvider initialState={createInitialAppState("ja-JP")}>
      <Harness tab={tab} />
    </AppStateProvider>,
  );
}

async function emitProgress(payload: JobProgressPayload) {
  await act(() => emit(JOB_PROGRESS_EVENT, payload));
}

describe("JobFooter with useJobEvents and useJobRunner", () => {
  // Unmount first: unmounting unlistens, which needs the mocked events.
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("shows the idle status and the screen's action before any job", () => {
    mockCommands(() => undefined);
    renderHarness();

    expect(screen.getByText("idle")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "screen action" }),
    ).toBeInTheDocument();
  });

  it("sends the IDs in order and shows progress with the running items", async () => {
    const calls = mockCommands(() => undefined);
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "start each" }));
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "start_images_to_pdfs",
        args: { ids: [1, 2, 3], pageSize: "fit" },
      }),
    );
    expect(
      screen.queryByRole("button", { name: "screen action" }),
    ).not.toBeInTheDocument();

    await emitProgress({ done: 0, total: 3, current: "a.png" });
    expect(screen.getByText("変換中：a.png")).toBeInTheDocument();

    await emitProgress({ done: 0, total: 3, current: "b.png" });
    expect(screen.getByText("変換中：b.png ほか 1 件")).toBeInTheDocument();

    await act(() =>
      emit(JOB_ITEM_EVENT, { id: 1, status: "ok", outputs: ["a.pdf"] }),
    );
    await emitProgress({ done: 1, total: 3, current: null });
    expect(screen.getByText("変換中：b.png")).toBeInTheDocument();
    expect(screen.getByText("1 / 3")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "進捗" })).toHaveAttribute(
      "aria-valuenow",
      "1",
    );
  });

  it("keeps events that arrive before the start command resolves", async () => {
    let resolveStart: (value: undefined) => void = () => {};
    mockCommands((cmd) =>
      cmd === "start_images_to_pdfs"
        ? new Promise((resolve) => {
            resolveStart = resolve;
          })
        : undefined,
    );
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "start each" }));
    await emitProgress({ done: 0, total: 3, current: "a.png" });
    expect(screen.getByText("変換中：a.png")).toBeInTheDocument();

    await act(async () => resolveStart(undefined));
    expect(screen.getByText("変換中：a.png")).toBeInTheDocument();
  });

  it("shows キャンセル中… at once and asks the backend to cancel", async () => {
    const calls = mockCommands(() => undefined);
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "start each" }));
    await emitProgress({ done: 0, total: 3, current: "a.png" });
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));

    const button = screen.getByRole("button", { name: "キャンセル中…" });
    expect(button).toBeDisabled();
    expect(screen.getAllByText("キャンセル中…")).toHaveLength(2);
    await waitFor(() =>
      expect(calls.map((call) => call.cmd)).toContain("cancel_job"),
    );
  });

  it("shows 完了 and the screen's action again when the job finishes", async () => {
    mockCommands(() => undefined);
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "start each" }));
    await act(() => emit(JOB_FINISHED_EVENT, finished));

    expect(screen.getByText("完了")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "screen action" }),
    ).toBeInTheDocument();
  });

  it("shows a finished job only on the tab that started it", async () => {
    mockCommands(() => undefined);
    renderHarness("imagesToPdf");

    fireEvent.click(screen.getByRole("button", { name: "start pdfs" }));
    await act(() => emit(JOB_FINISHED_EVENT, finished));

    expect(screen.getByText("idle")).toBeInTheDocument();
    expect(screen.queryByText("完了")).not.toBeInTheDocument();
  });

  it("sends the PDF → image arguments", async () => {
    const calls = mockCommands(() => undefined);
    renderHarness("pdfToImages");

    fireEvent.click(screen.getByRole("button", { name: "start pdfs" }));
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "start_pdfs_to_images",
        args: { ids: [7], range: "1-2", format: "jpeg", dpi: 300 },
      }),
    );
  });

  it("returns to idle when the save dialog is cancelled", async () => {
    mockCommands((cmd) => (cmd === "save_merged_pdf" ? null : undefined));
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save merged" }));

    await waitFor(() => expect(screen.getByText("idle")).toBeInTheDocument());
    expect(screen.getByTestId("saved-name")).toHaveTextContent("");
  });

  it("records the saved name of a merged PDF", async () => {
    const calls = mockCommands((cmd) =>
      cmd === "save_merged_pdf" ? { savedName: "a.pdf" } : undefined,
    );
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "save merged" }));

    await waitFor(() =>
      expect(screen.getByTestId("saved-name")).toHaveTextContent("a.pdf"),
    );
    expect(calls).toContainEqual({
      cmd: "save_merged_pdf",
      args: { ids: [1, 2, 3], pageSize: "a4" },
    });
  });

  it("shows why a conversion could not start", async () => {
    mockCommands((cmd) => {
      if (cmd === "start_images_to_pdfs") {
        throw { code: "ConversionRunning", detail: null };
      }
      return undefined;
    });
    renderHarness();

    fireEvent.click(screen.getByRole("button", { name: "start each" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "変換中は操作できません",
    );
    expect(
      screen.getByRole("button", { name: "screen action" }),
    ).toBeInTheDocument();
  });
});
