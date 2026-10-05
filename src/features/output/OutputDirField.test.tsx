import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OutputDirLabel } from "../../ipc";
import { AppStateProvider, createInitialAppState } from "../../state";
import { OutputDirField } from "./OutputDirField";

interface Call {
  cmd: string;
  args: unknown;
}

function mockPick(answer: () => unknown): Call[] {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    return answer();
  });
  return calls;
}

function renderField(
  value: OutputDirLabel | null,
  onChange: (value: OutputDirLabel) => void = () => {},
  disabled = false,
) {
  return render(
    <AppStateProvider initialState={createInitialAppState("ja-JP")}>
      <OutputDirField
        kind="pdfToImages"
        value={value}
        onChange={onChange}
        disabled={disabled}
        hint="screen hint"
      />
    </AppStateProvider>,
  );
}

describe("OutputDirField", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("says no folder is chosen yet", () => {
    mockPick(() => null);
    renderField(null);

    expect(screen.getByText("保存先フォルダ")).toBeInTheDocument();
    expect(screen.getByTestId("output-dir-name")).toHaveTextContent("未選択");
    expect(screen.getByText("screen hint")).toBeInTheDocument();
  });

  it("shows the folder's name", () => {
    mockPick(() => null);
    renderField({ dirLabel: "invoices-images" });

    expect(screen.getByTestId("output-dir-name")).toHaveTextContent(
      "invoices-images",
    );
  });

  it("asks Rust for a folder of its kind and reports the pick", async () => {
    const calls = mockPick(() => ({ dirLabel: "out" }));
    const onChange = vi.fn();
    renderField(null, onChange);

    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));

    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith({ dirLabel: "out" }),
    );
    expect(calls).toEqual([
      { cmd: "pick_output_dir", args: { kind: "pdfToImages" } },
    ]);
  });

  it("reports nothing when the dialog is cancelled", async () => {
    const calls = mockPick(() => null);
    const onChange = vi.fn();
    renderField(null, onChange);

    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));

    await waitFor(() => expect(calls).toHaveLength(1));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows why the dialog could not open", async () => {
    mockPick(() => {
      throw { code: "ReadFailed", detail: "out" };
    });
    renderField(null);

    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  it("cannot be used while disabled", () => {
    mockPick(() => null);
    renderField(null, () => {}, true);

    expect(
      screen.getByRole("button", { name: "フォルダを選ぶ" }),
    ).toBeDisabled();
  });
});
