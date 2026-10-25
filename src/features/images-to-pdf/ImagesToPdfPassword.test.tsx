import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ImageItem } from "../../ipc";
import { AppStateProvider, createInitialAppState } from "../../state";
import { JobFooter } from "../job/JobFooter";
import { useJobEvents } from "../job/useJobEvents";
import { ImagesToPdfAction } from "./ImagesToPdfAction";
import { ImagesToPdfSettings } from "./ImagesToPdfSettings";
import { ImagesToPdfStatus } from "./ImagesToPdfStatus";
import { ImagesToPdfView } from "./ImagesToPdfView";

const PASSWORD = "U15 pass-word!";

function image(id: number, name: string, pageCount = 1): ImageItem {
  return {
    id,
    name,
    width: 100,
    height: 100,
    format: "png",
    pageCount,
    bytes: 1000,
    error: null,
  };
}

interface IpcCall {
  cmd: string;
  args: unknown;
}

/** Answers add_images with `added` and records every command. */
function mockIpc(added: ImageItem[]): IpcCall[] {
  const calls: IpcCall[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === "add_images") {
        return {
          added,
          skipped: { unsupported: 0, folders: 0, duplicates: 0 },
        };
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "out" };
      }
      if (cmd === "save_merged_pdf") {
        return { savedName: "merged.pdf" };
      }
      if (cmd === "get_thumbnail") {
        return [137, 80, 78, 71];
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return calls;
}

function Harness() {
  useJobEvents();
  return (
    <div>
      <aside>
        <ImagesToPdfSettings />
      </aside>
      <main>
        <ImagesToPdfView />
      </main>
      <footer>
        <JobFooter
          tab="imagesToPdf"
          idleStatus={<ImagesToPdfStatus />}
          action={<ImagesToPdfAction />}
        />
      </footer>
    </div>
  );
}

async function renderWithImages(lang = "ja-JP") {
  render(
    <AppStateProvider initialState={createInitialAppState(lang)}>
      <Harness />
    </AppStateProvider>,
  );
  fireEvent.click(
    screen.getByRole("button", {
      name: lang === "ja-JP" ? "画像を追加" : "Add images",
    }),
  );
  await screen.findByText("a.png");
}

const encryptButton = () => screen.getByRole("button", { name: "パスワード" });
const pdfAButton = () => screen.getByRole("button", { name: "PDF/A" });
const standardButton = () => screen.getByRole("button", { name: "標準" });

function fillDialog(password = PASSWORD) {
  fireEvent.change(screen.getByLabelText("パスワード"), {
    target: { value: password },
  });
  fireEvent.change(screen.getByLabelText("もう一度入力"), {
    target: { value: password },
  });
}

function calledCommands(calls: IpcCall[]): string[] {
  return calls.map((call) => call.cmd);
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the PDF format", () => {
  it("starts at standard with no note, and shows each choice's note only while chosen", async () => {
    mockIpc([image(1, "a.png")]);
    await renderWithImages();

    expect(standardButton()).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.queryByText("保存時にパスワードを入れます"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText("CMYK の画像は使えません"),
    ).not.toBeInTheDocument();

    fireEvent.click(encryptButton());
    expect(encryptButton()).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("保存時にパスワードを入れます")).toHaveClass(
      "hint",
    );
    expect(
      screen.queryByText("CMYK の画像は使えません"),
    ).not.toBeInTheDocument();

    // Choosing PDF/A takes the password off: the two are one choice.
    fireEvent.click(pdfAButton());
    expect(pdfAButton()).toHaveAttribute("aria-pressed", "true");
    expect(encryptButton()).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByText("CMYK の画像は使えません")).toHaveClass("hint");
    expect(
      screen.queryByText("保存時にパスワードを入れます"),
    ).not.toBeInTheDocument();

    fireEvent.click(standardButton());
    expect(standardButton()).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.queryByText("CMYK の画像は使えません"),
    ).not.toBeInTheDocument();
  });

  it("offers the three choices in one group", async () => {
    mockIpc([image(1, "a.png")]);
    await renderWithImages();
    const group = screen.getByRole("group", { name: "PDF の形式" });
    expect(
      within(group)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["標準", "PDF/A", "パスワード"]);
    expect(screen.queryAllByRole("checkbox")).toHaveLength(0);
  });

  it("says it in English", async () => {
    mockIpc([image(1, "a.png")]);
    await renderWithImages("en-US");

    const group = screen.getByRole("group", { name: "PDF format" });
    expect(
      within(group)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Standard", "PDF/A", "Password"]);
    fireEvent.click(within(group).getByRole("button", { name: "Password" }));
    expect(
      screen.getByText("You'll enter the password when saving"),
    ).toBeInTheDocument();
  });
});

describe("the footer's line", () => {
  it("adds the mark after the preview only while the password is chosen", async () => {
    mockIpc([image(1, "a.png"), image(2, "b.png", 2)]);
    await renderWithImages();

    expect(screen.getByText("3 ページの PDF になります")).toBeInTheDocument();
    expect(screen.queryByText("パスワード付き")).not.toBeInTheDocument();

    fireEvent.click(encryptButton());
    const mark = screen.getByText("パスワード付き");
    expect(mark.textContent).toContain("·");
    expect(mark.querySelector("svg")).not.toBeNull();
    // One element holds both, so the footer's gap does not come between them.
    const preview = screen.getByText("3 ページの PDF になります");
    expect(preview).toHaveClass("hint");
    expect(mark.parentElement).toBe(preview);
    expect(
      preview.compareDocumentPosition(mark) &
        Node.DOCUMENT_POSITION_CONTAINED_BY,
    ).toBeTruthy();

    fireEvent.click(standardButton());
    expect(screen.queryByText("パスワード付き")).not.toBeInTheDocument();
  });

  it("follows the count of PDFs for one at a time, in English", async () => {
    mockIpc([image(1, "a.png"), image(2, "b.png")]);
    await renderWithImages("en-US");
    fireEvent.click(screen.getByRole("button", { name: "Each image" }));
    fireEvent.click(screen.getByRole("button", { name: "Password" }));

    expect(screen.getByText("Will save 2 PDFs")).toBeInTheDocument();
    expect(screen.getByText("Password-protected")).toBeInTheDocument();
  });

  it("does not follow a line that is not a preview", () => {
    mockIpc([]);
    render(
      <AppStateProvider initialState={createInitialAppState("ja-JP")}>
        <Harness />
      </AppStateProvider>,
    );
    fireEvent.click(encryptButton());
    expect(screen.getByText("画像が選ばれていません")).toBeInTheDocument();
    expect(screen.queryByText("パスワード付き")).not.toBeInTheDocument();
  });

  it("does not show the mark once a conversion has started", async () => {
    mockIpc([image(1, "a.png")]);
    await renderWithImages();
    fireEvent.click(encryptButton());
    expect(screen.getByText("パスワード付き")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    fillDialog();
    fireEvent.click(screen.getByRole("button", { name: "保存先を選ぶ…" }));

    await waitFor(() => {
      expect(screen.queryByText("パスワード付き")).not.toBeInTheDocument();
    });
  });
});

describe("starting with a password", () => {
  it("asks for the password first and saves one PDF with it", async () => {
    const calls = mockIpc([image(1, "a.png")]);
    await renderWithImages();
    fireEvent.click(encryptButton());

    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    const dialog = await screen.findByRole("dialog", {
      name: "パスワードを付ける",
    });
    expect(dialog).toBeInTheDocument();
    expect(calledCommands(calls)).not.toContain("save_merged_pdf");

    fillDialog();
    fireEvent.click(screen.getByRole("button", { name: "保存先を選ぶ…" }));

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "save_merged_pdf",
        args: {
          ids: [1],
          pageSize: "fit",
          orientation: "auto",
          pdfA: false,
          password: PASSWORD,
        },
      });
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks for the password, then the folder, then converts one at a time", async () => {
    const calls = mockIpc([image(1, "a.png"), image(2, "b.png")]);
    await renderWithImages();
    fireEvent.click(screen.getByRole("button", { name: "1 枚ずつ" }));
    fireEvent.click(encryptButton());

    fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));
    await screen.findByRole("dialog", { name: "パスワードを付ける" });
    expect(
      screen.getByText(
        "保存する 2 個の PDF すべてに、同じパスワードを付けます。",
      ),
    ).toBeInTheDocument();
    // No folder is chosen yet, and the dialog comes before that question.
    expect(calledCommands(calls)).not.toContain("pick_output_dir");

    fillDialog();
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "変換を開始",
      }),
    );

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_images_to_pdfs",
        args: {
          ids: [1, 2],
          pageSize: "fit",
          orientation: "auto",
          pdfA: false,
          password: PASSWORD,
        },
      });
    });
    const order = calledCommands(calls);
    expect(order.indexOf("pick_output_dir")).toBeLessThan(
      order.indexOf("start_images_to_pdfs"),
    );
  });

  it("starts nothing when the dialog is cancelled, and gives the focus back", async () => {
    const calls = mockIpc([image(1, "a.png")]);
    await renderWithImages();
    fireEvent.click(encryptButton());
    const action = screen.getByRole("button", { name: "PDF を保存" });

    fireEvent.click(action);
    await screen.findByRole("dialog");
    fillDialog();
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(action).toHaveFocus();
    expect(calledCommands(calls)).not.toContain("save_merged_pdf");

    // Escape cancels in the same way.
    fireEvent.click(action);
    await screen.findByRole("dialog");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(action).toHaveFocus();
    expect(calledCommands(calls)).not.toContain("save_merged_pdf");
    expect(calledCommands(calls)).not.toContain("pick_output_dir");
  });

  it("opens the dialog empty each time", async () => {
    mockIpc([image(1, "a.png")]);
    await renderWithImages();
    fireEvent.click(encryptButton());
    const action = screen.getByRole("button", { name: "PDF を保存" });

    fireEvent.click(action);
    await screen.findByRole("dialog");
    fillDialog();
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));

    fireEvent.click(action);
    await screen.findByRole("dialog");
    expect(screen.getByLabelText("パスワード")).toHaveValue("");
    expect(screen.getByLabelText("もう一度入力")).toHaveValue("");
  });

  it("sends no password, and opens no dialog, while the box is off", async () => {
    const calls = mockIpc([image(1, "a.png")]);
    await renderWithImages();

    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "save_merged_pdf",
        args: {
          ids: [1],
          pageSize: "fit",
          orientation: "auto",
          pdfA: false,
          password: null,
        },
      });
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks in English too", async () => {
    mockIpc([image(1, "a.png")]);
    await renderWithImages("en-US");
    fireEvent.click(screen.getByRole("button", { name: "Password" }));
    fireEvent.click(screen.getByRole("button", { name: "Save PDF" }));
    expect(
      await screen.findByRole("dialog", { name: "Set a password" }),
    ).toBeInTheDocument();
  });
});
