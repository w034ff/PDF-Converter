import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";

function renderDialog(detail?: string) {
  const onCancel = vi.fn();
  const onConfirm = vi.fn();
  render(
    <ConfirmDialog
      title="Remove everything?"
      detail={detail}
      cancelLabel="Cancel"
      confirmLabel="Remove"
      onCancel={onCancel}
      onConfirm={onConfirm}
    >
      This cannot be undone.
    </ConfirmDialog>,
  );
  return { onCancel, onConfirm };
}

describe("ConfirmDialog", () => {
  it("is named by its title and starts with the focus on cancel", () => {
    renderDialog();

    expect(
      screen.getByRole("dialog", { name: "Remove everything?" }),
    ).toBeTruthy();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "Cancel" }),
    );
  });

  it("shows the detail only when it has one", () => {
    renderDialog("report.pdf");

    expect(screen.getByText("report.pdf")).toBeTruthy();
  });

  it("confirms with the confirm button alone", () => {
    const { onCancel, onConfirm } = renderDialog();

    fireEvent.click(screen.getByRole("button", { name: "Remove" }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("cancels with the cancel button and with Esc", () => {
    const { onCancel, onConfirm } = renderDialog();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.keyDown(window, { key: "Escape" });

    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
