import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi, type Mock } from "vitest";
import { AppStateProvider, createInitialAppState } from "../../state";
import { OutputPasswordDialog } from "./OutputPasswordDialog";

interface Callbacks {
  onCancel: Mock<() => void>;
  onConfirm: Mock<(password: string) => void>;
}

function renderDialog(
  options: { mode?: "merge" | "each"; count?: number; lang?: string } = {},
): Callbacks {
  const callbacks: Callbacks = {
    onCancel: vi.fn<() => void>(),
    onConfirm: vi.fn<(password: string) => void>(),
  };
  render(
    <AppStateProvider
      initialState={createInitialAppState(options.lang ?? "ja-JP")}
    >
      <OutputPasswordDialog
        isOpen={true}
        mode={options.mode ?? "merge"}
        count={options.count ?? 1}
        {...callbacks}
      />
    </AppStateProvider>,
  );
  return callbacks;
}

const passwordField = () => screen.getByLabelText("パスワード");
const confirmField = () => screen.getByLabelText("もう一度入力");
const primaryButton = (name = "保存先を選ぶ…") =>
  screen.getByRole("button", { name });

function submit() {
  const form = passwordField().closest("form");
  if (form === null) {
    throw new Error("the dialog has no form");
  }
  fireEvent.submit(form);
}

function type(field: HTMLElement, value: string) {
  fireEvent.change(field, { target: { value } });
}

function fill(password: string, confirmation = password) {
  type(passwordField(), password);
  type(confirmField(), confirmation);
}

/** The line of the conditions list that says `label`. */
function condition(label: string): HTMLElement {
  const item = screen.getByText(label, { exact: false }).closest("li");
  if (item === null) {
    throw new Error(`no condition says ${label}`);
  }
  return item;
}

const CHARACTERS = "半角の英数字と記号";
const MATCH = "パスワードが一致";

afterEach(() => {
  cleanup();
});

describe("OutputPasswordDialog", () => {
  it("opens on the password field, empty, with every condition pending", () => {
    renderDialog();

    expect(screen.getByRole("dialog", { name: "パスワードを付ける" }));
    expect(passwordField()).toHaveFocus();
    expect(passwordField()).toHaveAttribute("type", "password");
    expect(confirmField()).toHaveAttribute("type", "password");
    expect(passwordField()).toHaveAttribute("autocomplete", "off");
    expect(passwordField()).not.toHaveAttribute("maxlength");
    expect(confirmField()).not.toHaveAttribute("maxlength");
    for (const label of [CHARACTERS, MATCH]) {
      expect(condition(label)).toHaveClass("is-pending");
      expect(condition(label)).toHaveTextContent("○");
    }
    expect(primaryButton()).toBeDisabled();
  });

  it("says nothing under the heading for one PDF", () => {
    renderDialog({ mode: "merge" });
    const heading = screen.getByRole("heading", { name: "パスワードを付ける" });
    expect(heading.nextSibling).toBeNull();
    expect(
      screen.getByText(
        "パスワードを忘れると、この PDF はだれも開けなくなります。",
      ),
    ).toBeInTheDocument();
  });

  it("says that every PDF gets the same password for one at a time", () => {
    renderDialog({ mode: "each", count: 3 });
    expect(
      screen.getByText(
        "保存する 3 個の PDF すべてに、同じパスワードを付けます。",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "パスワードを忘れると、これらの PDF はだれも開けなくなります。",
      ),
    ).toBeInTheDocument();
    expect(primaryButton("変換を開始")).toBeDisabled();
  });

  it("marks the characters met for ASCII and failed for anything else", () => {
    renderDialog();

    type(passwordField(), "abc DEF-123!");
    expect(condition(CHARACTERS)).toHaveClass("is-met");
    expect(condition(CHARACTERS)).toHaveTextContent("✓");
    expect(condition(CHARACTERS)).toHaveTextContent("満たしています");
    expect(passwordField()).not.toHaveAttribute("aria-describedby");

    type(passwordField(), "abc全角");
    expect(condition(CHARACTERS)).toHaveClass("is-failed");
    expect(condition(CHARACTERS)).toHaveTextContent("✕");
    expect(condition(CHARACTERS)).toHaveTextContent("満たしていません");
    expect(passwordField()).toHaveAttribute(
      "aria-describedby",
      condition(CHARACTERS).id,
    );

    type(passwordField(), "");
    expect(condition(CHARACTERS)).toHaveClass("is-pending");
  });

  it("does not turn the match red while the confirmation is a start of the password", () => {
    renderDialog();
    type(passwordField(), "secret");

    type(confirmField(), "sec");
    expect(condition(MATCH)).toHaveClass("is-pending");
    expect(confirmField()).not.toHaveAttribute("aria-describedby");

    type(confirmField(), "seX");
    expect(condition(MATCH)).toHaveClass("is-failed");
    expect(condition(MATCH)).toHaveTextContent("満たしていません");
    expect(confirmField()).toHaveAttribute(
      "aria-describedby",
      condition(MATCH).id,
    );

    type(confirmField(), "secrets");
    expect(condition(MATCH)).toHaveClass("is-failed");

    type(confirmField(), "secret");
    expect(condition(MATCH)).toHaveClass("is-met");
    expect(condition(MATCH)).toHaveTextContent("満たしています");

    type(confirmField(), "");
    expect(condition(MATCH)).toHaveClass("is-pending");
  });

  it("does not use a live region for the conditions", () => {
    renderDialog();
    const list = condition(CHARACTERS).closest("ul");
    expect(list).not.toBeNull();
    expect(list).not.toHaveAttribute("aria-live");
    expect(list?.closest("[aria-live]")).toBeNull();
  });

  it("enables the button only when both conditions are met", () => {
    renderDialog();

    fill("secret", "");
    expect(primaryButton()).toBeDisabled();
    fill("secret", "sec");
    expect(primaryButton()).toBeDisabled();
    fill("secret", "secreT");
    expect(primaryButton()).toBeDisabled();
    fill("全角のパスワード");
    expect(primaryButton()).toBeDisabled();
    fill("secret");
    expect(primaryButton()).toBeEnabled();
    fill(" ");
    expect(primaryButton()).toBeEnabled();
  });

  it("accepts 127 characters and refuses 128 with a note under the field", () => {
    renderDialog();

    fill("a".repeat(127));
    expect(primaryButton()).toBeEnabled();
    expect(screen.queryByText("127 文字までです")).not.toBeInTheDocument();

    fill("a".repeat(128));
    expect(primaryButton()).toBeDisabled();
    const note = screen.getByText("127 文字までです");
    expect(note).toHaveClass("error-text");
    expect(passwordField()).toHaveAttribute("aria-describedby", note.id);
    // The limit is not in the list, and what was pasted is not cut.
    expect(condition(CHARACTERS)).not.toHaveTextContent("127");
    expect(passwordField()).toHaveValue("a".repeat(128));
  });

  it("describes both the characters and the length when both fail", () => {
    renderDialog();
    fill("あ".repeat(128));
    const ids = (passwordField().getAttribute("aria-describedby") ?? "").split(
      " ",
    );
    expect(ids).toContain(condition(CHARACTERS).id);
    expect(ids).toContain(screen.getByText("127 文字までです").id);
  });

  it("shows and hides both fields with the one button", () => {
    renderDialog();
    const toggle = screen.getByRole("button", { name: "表示" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(toggle);
    expect(passwordField()).toHaveAttribute("type", "text");
    expect(confirmField()).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "隠す" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "隠す" }));
    expect(passwordField()).toHaveAttribute("type", "password");
    expect(confirmField()).toHaveAttribute("type", "password");
  });

  it("confirms with Enter when the button is enabled", () => {
    const { onConfirm } = renderDialog();

    fill("secret", "sec");
    submit();
    expect(onConfirm).not.toHaveBeenCalled();

    fill("secret");
    submit();
    expect(onConfirm).toHaveBeenCalledExactlyOnceWith("secret");
  });

  it("confirms with the password through the button", () => {
    const { onConfirm, onCancel } = renderDialog();
    fill("U15 pass-word!");
    fireEvent.click(primaryButton());
    expect(onConfirm).toHaveBeenCalledExactlyOnceWith("U15 pass-word!");
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("cancels with Escape and with the button, and confirms nothing", () => {
    const { onConfirm, onCancel } = renderDialog();
    fill("secret");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("is empty again when it is opened after being closed, with the fields hidden", () => {
    function Host() {
      const [open, setOpen] = useState(true);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            open
          </button>
          <OutputPasswordDialog
            isOpen={open}
            mode="merge"
            count={1}
            onCancel={() => setOpen(false)}
            onConfirm={() => setOpen(false)}
          />
        </>
      );
    }
    render(
      <AppStateProvider initialState={createInitialAppState("ja-JP")}>
        <Host />
      </AppStateProvider>,
    );

    fill("secret");
    fireEvent.click(screen.getByRole("button", { name: "表示" }));
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "open" }));
    expect(passwordField()).toHaveValue("");
    expect(confirmField()).toHaveValue("");
    expect(passwordField()).toHaveAttribute("type", "password");
    expect(primaryButton()).toBeDisabled();
  });

  it("says it in English, with the singular for one PDF", () => {
    renderDialog({ mode: "each", count: 1, lang: "en-US" });
    const dialog = screen.getByRole("dialog", { name: "Set a password" });
    expect(
      within(dialog).getByText("The PDF will get this password."),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "If you forget the password, no one can open these PDFs.",
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Password")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Enter it again")).toBeInTheDocument();
    expect(
      within(dialog).getByText("ASCII letters, numbers and symbols"),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("Passwords match")).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "Start converting" }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: "Show" }),
    ).toBeInTheDocument();
  });

  it("says the count for several PDFs and the way to save for one, in English", () => {
    cleanup();
    renderDialog({ mode: "each", count: 4, lang: "en-US" });
    expect(
      screen.getByText("All 4 PDFs will get the same password."),
    ).toBeInTheDocument();

    cleanup();
    renderDialog({ mode: "merge", lang: "en-US" });
    expect(
      screen.getByText("If you forget the password, no one can open the PDF."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Choose where to save…" }),
    ).toBeInTheDocument();
    type(screen.getByLabelText("Password"), "a".repeat(128));
    expect(screen.getByText("Use 127 characters or fewer")).toBeInTheDocument();
  });
});
