import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppStateProvider, createInitialAppState } from "../../state";
import { useEnsureOutputDir } from "./useEnsureOutputDir";

function createWrapper() {
  const initial = createInitialAppState("ja-JP");
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <AppStateProvider initialState={initial}>{children}</AppStateProvider>
    );
  };
}

describe("useEnsureOutputDir", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("calls onConfirmed directly when outputDir is already set", async () => {
    let pickCalled = false;
    mockIPC((cmd) => {
      if (cmd === "pick_output_dir") {
        pickCalled = true;
      }
      return undefined;
    });

    const onConfirmed = vi.fn();
    const { result } = renderHook(() => useEnsureOutputDir("imagesToPdf"), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.runWithOutputDir(
        { dirLabel: "already" },
        onConfirmed,
      );
    });

    expect(onConfirmed).toHaveBeenCalledTimes(1);
    expect(pickCalled).toBe(false);
  });

  it("picks output dir and calls onConfirmed when outputDir is null", async () => {
    mockIPC((cmd) => {
      if (cmd === "pick_output_dir") {
        return { dirLabel: "newly-picked" };
      }
      return undefined;
    });

    const onConfirmed = vi.fn();
    const { result } = renderHook(() => useEnsureOutputDir("imagesToPdf"), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.runWithOutputDir(null, onConfirmed);
    });

    expect(onConfirmed).toHaveBeenCalledTimes(1);
  });

  it("does not call onConfirmed when pick_output_dir is cancelled", async () => {
    mockIPC((cmd) => {
      if (cmd === "pick_output_dir") {
        return null;
      }
      return undefined;
    });

    const onConfirmed = vi.fn();
    const { result } = renderHook(() => useEnsureOutputDir("imagesToPdf"), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.runWithOutputDir(null, onConfirmed);
    });

    expect(onConfirmed).not.toHaveBeenCalled();
  });

  it("does not call onConfirmed when pick_output_dir throws", async () => {
    mockIPC((cmd) => {
      if (cmd === "pick_output_dir") {
        throw { code: "Failed", detail: "mock error" };
      }
      return undefined;
    });

    const onConfirmed = vi.fn();
    const { result } = renderHook(() => useEnsureOutputDir("imagesToPdf"), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.runWithOutputDir(null, onConfirmed);
    });

    expect(onConfirmed).not.toHaveBeenCalled();
  });
});
