import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppStateProvider, createInitialAppState } from "../../state";
import { PasswordDialog } from "./PasswordDialog";

afterEach(cleanup);

function renderDialog(unlock: (password: string) => Promise<string>) {
  render(
    <AppStateProvider initialState={createInitialAppState("ja-JP")}>
      <PasswordDialog
        name="locked.pdf"
        unlock={unlock}
        onClose={vi.fn()}
        onSuccess={vi.fn()}
      />
    </AppStateProvider>,
  );
}

function rejectWrongPassword(): Promise<string> {
  return Promise.reject({ code: "PasswordProtected", detail: null });
}

function passwordField(): HTMLInputElement {
  const field: unknown = screen.getByLabelText("パスワード");
  if (!(field instanceof HTMLInputElement)) {
    throw new Error("the password field is not an input");
  }
  return field;
}

async function submitWrongPassword() {
  fireEvent.change(passwordField(), { target: { value: "wrong" } });
  fireEvent.click(screen.getByRole("button", { name: "開く" }));
  await screen.findByRole("alert");
}

describe("PasswordDialog", () => {
  it("selects the input again after a wrong password", async () => {
    renderDialog(rejectWrongPassword);

    await submitWrongPassword();

    await waitFor(() => {
      expect(document.activeElement).toBe(passwordField());
    });
    expect(screen.getByRole("alert").textContent).toBe("パスワードが違います");
    expect(passwordField().selectionStart).toBe(0);
    expect(passwordField().selectionEnd).toBe("wrong".length);
  });

  it("selects the input again after every wrong password", async () => {
    renderDialog(rejectWrongPassword);

    await submitWrongPassword();
    screen.getByRole("button", { name: "表示" }).focus();
    expect(document.activeElement).not.toBe(passwordField());
    await submitWrongPassword();

    await waitFor(() => {
      expect(document.activeElement).toBe(passwordField());
    });
  });
});
