import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FINISHED_NONE,
  NO_SKIPPED,
  PASSWORD,
  addThroughMenu,
  added,
  brokenPdf,
  callsOf,
  drawnKeys,
  emitJobFinished,
  emitOrganizeDropped,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openOrganizeTab,
  organizePdf,
  waitingPdf,
} from "./organizeTestSupport";

beforeEach(() => {
  installFakeResizeObserver(700, 600);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const REPORT = organizePdf(1, "annual-report.pdf", 3);
const CONTRACT = organizePdf(2, "contract.pdf", 2, { encrypted: true });

describe("the organize tab: opening it", () => {
  it("is the fourth tab and tells Rust that drops come to it", async () => {
    const calls = await openOrganizeTab();

    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "画像 → PDF",
      "PDF → 画像",
      "切り出し",
      "整理",
    ]);
    expect(screen.getByRole("tab", { name: "整理" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      callsOf(calls, "set_drop_target").map((call) => call.args.target),
    ).toEqual(["lists", "organize"]);
  });

  it("is called Organize in English", async () => {
    await openOrganizeTab({ language: "en" });

    expect(screen.getByRole("tab", { name: "Organize" })).toBeInTheDocument();
    expect(
      screen.getByText("Drop PDFs here, or press “Add PDFs”"),
    ).toBeInTheDocument();
  });

  it("shows the drop zone with its two buttons, and the settings without the choice of how to save", async () => {
    await openOrganizeTab();

    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "PDF を追加" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "フォルダを追加" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    ).not.toBeChecked();
    expect(screen.getByText("保存した PDF について")).toBeInTheDocument();
    expect(screen.queryByText("保存の方法")).not.toBeInTheDocument();
    expect(screen.queryByText(/並んだ順に/)).not.toBeInTheDocument();
    expect(screen.getByText("PDF を追加してください")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "PDF を保存" })).toBeDisabled();
  });

  it("sends a drop to the list of the organize tab only while it is shown", async () => {
    const calls = await openOrganizeTab();

    fireEvent.click(screen.getByRole("tab", { name: "切り出し" }));
    await flush();

    expect(
      callsOf(calls, "set_drop_target").map((call) => call.args.target),
    ).toEqual(["lists", "organize", "crop"]);
  });
});

describe("adding PDFs", () => {
  it("asks for files, and shows a chip for each PDF and its pages in the grid, in order", async () => {
    const calls = await openOrganizeTab({ pdfs: [REPORT, CONTRACT] });

    expect(callsOf(calls, "add_organize_pdfs")[0].args).toEqual({
      source: "files",
    });
    const chips = within(
      screen.getByRole("list", { name: "PDF" }),
    ).getAllByRole("listitem");
    expect(chips.map((chip) => chip.textContent)).toEqual([
      "annual-report.pdf3 ページ✕",
      "contract.pdf2 ページ✕",
    ]);
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3", "2:1", "2:2"]);
    expect(screen.getByText("5 ページ")).toBeInTheDocument();
    expect(screen.queryByTestId("drop-zone")).not.toBeInTheDocument();
  });

  it("marks a PDF that needed a password with a key, and counts the pages the PDF has", async () => {
    await openOrganizeTab({ pdfs: [REPORT, CONTRACT] });

    const chips = screen.getAllByRole("listitem");
    expect(within(chips[0]).queryByRole("img")).not.toBeInTheDocument();
    expect(
      within(chips[1]).getByRole("img", { name: "パスワード付き" }),
    ).toBeInTheDocument();
    // The note under the settings tells that the passwords are not kept.
    expect(
      screen.getByText(
        "パスワード付きの PDF があります。元の PDF のパスワードは引き継ぎません。",
      ),
    ).toBeInTheDocument();
  });

  it("does not say so when no PDF needed a password", async () => {
    await openOrganizeTab({ pdfs: [REPORT] });

    expect(
      screen.queryByText(/元の PDF のパスワードは引き継ぎません/),
    ).not.toBeInTheDocument();
  });

  it("adds the folder from the empty state, and more files from the menu", async () => {
    const calls = await openOrganizeTab({
      handlers: {
        add_organize_pdfs: (args) =>
          args.source === "folder"
            ? added([REPORT])
            : added([organizePdf(5, "more.pdf", 1)]),
      },
    });

    fireEvent.click(screen.getByRole("button", { name: "フォルダを追加" }));
    await flush();
    await addThroughMenu();

    expect(
      callsOf(calls, "add_organize_pdfs").map((call) => call.args.source),
    ).toEqual(["folder", "files"]);
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3", "5:1"]);
  });

  it("changes nothing when the dialog is cancelled", async () => {
    await openOrganizeTab({ handlers: { add_organize_pdfs: () => null } });

    fireEvent.click(screen.getByRole("button", { name: "PDF を追加" }));
    await flush();

    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("tells what was left out, as the lists do", async () => {
    await openOrganizeTab({
      handlers: {
        add_organize_pdfs: () =>
          added([REPORT], { unsupported: 2, folders: 1, duplicates: 3 }),
      },
    });

    fireEvent.click(screen.getByRole("button", { name: "PDF を追加" }));
    await flush();

    expect(
      screen.getByText("（スキップ: 非対応 2 件、フォルダ 1 件、重複 3 件）"),
    ).toBeInTheDocument();
  });

  it("shows a failure to add in a band that can be closed", async () => {
    await openOrganizeTab({
      handlers: {
        add_organize_pdfs: () => {
          throw { code: "ConversionRunning", detail: null };
        },
      },
    });

    fireEvent.click(screen.getByRole("button", { name: "PDF を追加" }));
    await flush();

    const band = screen.getByRole("alert");
    expect(band).toHaveTextContent(/変換/);
    fireEvent.click(within(band).getByRole("button", { name: "閉じる" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("drops the note of an earlier addition when the next one has nothing to tell", async () => {
    let first = true;
    await openOrganizeTab({
      handlers: {
        add_organize_pdfs: () => {
          const result = first
            ? added([REPORT], { unsupported: 1, folders: 0, duplicates: 0 })
            : added([organizePdf(9, "x.pdf", 1)]);
          first = false;
          return result;
        },
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "PDF を追加" }));
    await flush();
    expect(screen.getByText(/スキップ/)).toBeInTheDocument();

    await addThroughMenu();

    expect(screen.queryByText(/スキップ/)).not.toBeInTheDocument();
  });
});

describe("removing a PDF", () => {
  it("asks Rust, then takes the chip and its pages away", async () => {
    const calls = await openOrganizeTab({ pdfs: [REPORT, CONTRACT] });

    fireEvent.click(
      screen.getByRole("button", { name: "annual-report.pdf を外す" }),
    );
    await flush();

    expect(callsOf(calls, "remove_organize_pdfs")[0].args).toEqual({
      ids: [1],
    });
    expect(drawnKeys()).toEqual(["2:1", "2:2"]);
    expect(screen.queryByText("annual-report.pdf")).not.toBeInTheDocument();
  });

  it("keeps the PDF when Rust refuses", async () => {
    await openOrganizeTab({
      pdfs: [REPORT],
      handlers: {
        remove_organize_pdfs: () => {
          throw { code: "ConversionRunning", detail: null };
        },
      },
    });

    fireEvent.click(
      screen.getByRole("button", { name: "annual-report.pdf を外す" }),
    );
    await flush();

    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("goes back to the drop zone when the last PDF is removed", async () => {
    await openOrganizeTab({ pdfs: [REPORT] });

    fireEvent.click(
      screen.getByRole("button", { name: "annual-report.pdf を外す" }),
    );
    await flush();

    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
  });
});

describe("a PDF that waits for a password", () => {
  const WAITING = waitingPdf(2, "locked.pdf");

  it("is a yellow chip with the button instead of a page count, and has no pages", async () => {
    await openOrganizeTab({ pdfs: [REPORT, WAITING] });

    const chip = screen.getAllByRole("listitem")[1];
    expect(chip).toHaveClass("is-waiting");
    expect(
      within(chip).getByRole("button", { name: "パスワードを入力" }),
    ).toBeInTheDocument();
    expect(within(chip).queryByText(/ページ/)).not.toBeInTheDocument();
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
    expect(
      screen.getByText(
        "3 ページを 1 つの PDF に保存します（約 3 MB）（1 個の PDF は保存に入りません）",
      ),
    ).toBeInTheDocument();
  });

  it("does not open the dialog by itself", async () => {
    await openOrganizeTab({ pdfs: [WAITING] });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("stays in the dialog on a wrong password and says so", async () => {
    const calls = await openOrganizeTab({
      pdfs: [REPORT, WAITING],
      handlers: {
        unlock_organize_pdf: () => {
          throw PASSWORD;
        },
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "パスワードを入力" }));
    const dialog = screen.getByRole("dialog");

    fireEvent.change(within(dialog).getByLabelText("パスワード"), {
      target: { value: "wrong" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "開く" }));
    await flush();

    expect(callsOf(calls, "unlock_organize_pdf")[0].args).toEqual({
      id: 2,
      password: "wrong",
    });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      within(dialog).getByText("パスワードが違います"),
    ).toBeInTheDocument();
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("opens every PDF the password opens and adds their pages at the end, the target first", async () => {
    const second = waitingPdf(3, "second.pdf");
    const stillLocked = waitingPdf(4, "other-password.pdf");
    await openOrganizeTab({
      pdfs: [REPORT, WAITING, second, stillLocked],
      handlers: {
        unlock_organize_pdf: () => ({
          items: [
            // The target first, then the others by ID.
            organizePdf(2, "locked.pdf", 2, { encrypted: true }),
            organizePdf(3, "second.pdf", 1, { encrypted: true }),
            stillLocked,
          ],
        }),
      },
    });
    fireEvent.click(
      screen.getAllByRole("button", { name: "パスワードを入力" })[0],
    );
    const dialog = screen.getByRole("dialog");

    fireEvent.change(within(dialog).getByLabelText("パスワード"), {
      target: { value: "right" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "開く" }));
    await flush();

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3", "2:1", "2:2", "3:1"]);
    const chips = screen.getAllByRole("listitem");
    expect(chips[1]).not.toHaveClass("is-waiting");
    expect(chips[1]).toHaveTextContent("2 ページ");
    expect(chips[3]).toHaveClass("is-waiting");
    // The key stays on the PDFs that needed the password, and the note about them stays.
    expect(
      within(chips[1]).getByRole("img", { name: "パスワード付き" }),
    ).toBeInTheDocument();
  });

  it("cannot be asked for while converting", async () => {
    await openOrganizeTab({
      pdfs: [REPORT, WAITING],
      handlers: { save_organized_pdf: () => new Promise(() => {}) },
    });

    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    await flush();

    expect(
      screen.getByRole("button", { name: "パスワードを入力" }),
    ).toBeDisabled();
  });
});

describe("a PDF that cannot be read", () => {
  it("is a red chip with the reason as its title, and no pages", async () => {
    await openOrganizeTab({ pdfs: [REPORT, brokenPdf(2, "broken.pdf")] });

    const chip = screen.getAllByRole("listitem")[1];
    expect(chip).toHaveClass("is-broken");
    expect(chip).toHaveTextContent("✕ 読み込めません");
    expect(chip.getAttribute("title")).toMatch(/PDF/);
    expect(
      within(chip).queryByRole("button", { name: "パスワードを入力" }),
    ).toBeNull();
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("is told in the bottom bar, with the PDFs that wait for a password", async () => {
    await openOrganizeTab({
      pdfs: [REPORT, brokenPdf(2, "broken.pdf"), waitingPdf(3, "locked.pdf")],
    });

    expect(
      screen.getByText(
        /3 ページを 1 つの PDF に保存します.*（2 個の PDF は保存に入りません）/,
      ),
    ).toBeInTheDocument();
  });

  it("can be removed with its ✕", async () => {
    const calls = await openOrganizeTab({
      pdfs: [REPORT, brokenPdf(2, "broken.pdf")],
    });

    fireEvent.click(screen.getByRole("button", { name: "broken.pdf を外す" }));
    await flush();

    expect(callsOf(calls, "remove_organize_pdfs")[0].args).toEqual({
      ids: [2],
    });
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });
});

describe("dropping PDFs", () => {
  it("adds what was dropped as the dialog does", async () => {
    await openOrganizeTab({ pdfs: [REPORT] });

    await emitOrganizeDropped({
      added: [organizePdf(7, "dropped.pdf", 2)],
      skipped: NO_SKIPPED,
      error: null,
    });

    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3", "7:1", "7:2"]);
  });

  it("starts from the drop zone", async () => {
    await openOrganizeTab();

    await emitOrganizeDropped({
      added: [REPORT],
      skipped: NO_SKIPPED,
      error: null,
    });

    expect(screen.queryByTestId("drop-zone")).not.toBeInTheDocument();
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("tells what was left out", async () => {
    await openOrganizeTab({ pdfs: [REPORT] });

    await emitOrganizeDropped({
      added: [],
      skipped: { unsupported: 0, folders: 0, duplicates: 2 },
      error: null,
    });

    expect(screen.getByText("（スキップ: 重複 2 件）")).toBeInTheDocument();
  });

  it("shows an error in a band", async () => {
    await openOrganizeTab({ pdfs: [REPORT] });

    await emitOrganizeDropped({
      added: [],
      skipped: NO_SKIPPED,
      error: { code: "ConversionRunning", detail: null },
    });

    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("is taken even when it comes while another tab is shown", async () => {
    await openOrganizeTab();
    fireEvent.click(screen.getByRole("tab", { name: "切り出し" }));
    await flush();

    await emitOrganizeDropped({
      added: [REPORT],
      skipped: NO_SKIPPED,
      error: null,
    });
    fireEvent.click(screen.getByRole("tab", { name: "整理" }));
    await flush();

    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3"]);
  });
});

describe("while converting", () => {
  it("cannot add, remove or clear", async () => {
    await openOrganizeTab({
      pdfs: [REPORT],
      handlers: { save_organized_pdf: () => new Promise(() => {}) },
    });
    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    await flush();

    expect(screen.getByRole("button", { name: "PDF を追加" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "annual-report.pdf を外す" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "すべて外す" })).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    ).toBeDisabled();
    await act(async () => {
      await emitJobFinished(FINISHED_NONE);
    });
  });
});
