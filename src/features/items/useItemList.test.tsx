import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AddResult, AddSource, Skipped } from "../../ipc";
import { useItemList } from "./useItemList";

interface Item {
  id: number;
}

const SKIPPED: Skipped = { unsupported: 1, folders: 0, duplicates: 2 };

function setup(
  items: Item[],
  add: (source: AddSource) => Promise<AddResult<Item> | null>,
) {
  const onAdded = vi.fn();
  const onRemoved = vi.fn();
  const onCleared = vi.fn();
  const { result } = renderHook(() =>
    useItemList({ items, add, onAdded, onRemoved, onCleared }),
  );
  return { result, onAdded, onRemoved, onCleared };
}

describe("useItemList", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("passes on what was added and keeps what was left out", async () => {
    const add = vi.fn(async () => ({ added: [{ id: 7 }], skipped: SKIPPED }));
    const { result, onAdded } = setup([], add);

    await act(() => result.current.add("folder"));

    expect(add).toHaveBeenCalledWith("folder");
    expect(onAdded).toHaveBeenCalledWith([{ id: 7 }]);
    expect(result.current.skipped).toEqual(SKIPPED);
  });

  it("keeps what was left out even when nothing was added", async () => {
    const { result, onAdded } = setup([], async () => ({
      added: [],
      skipped: SKIPPED,
    }));

    await act(() => result.current.add("files"));

    expect(onAdded).not.toHaveBeenCalled();
    expect(result.current.skipped).toEqual(SKIPPED);
  });

  it("changes nothing when the dialog is cancelled", async () => {
    const { result, onAdded } = setup([], async () => null);

    await act(() => result.current.add("files"));

    expect(onAdded).not.toHaveBeenCalled();
    expect(result.current.skipped).toBeNull();
    expect(result.current.error).toBeNull();
  });

  it("removes an item from the state only after Rust has", async () => {
    const calls: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "remove_items") {
        calls.push(args);
      }
    });
    const { result, onRemoved } = setup(
      [{ id: 1 }, { id: 2 }],
      async () => null,
    );

    await act(() => result.current.remove(2));

    expect(calls).toEqual([{ ids: [2] }]);
    expect(onRemoved).toHaveBeenCalledWith(2);
  });

  it("clears every item and forgets what was left out", async () => {
    const calls: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "remove_items") {
        calls.push(args);
      }
    });
    const { result, onCleared } = setup([{ id: 1 }, { id: 2 }], async () => ({
      added: [],
      skipped: SKIPPED,
    }));
    await act(() => result.current.add("files"));

    await act(() => result.current.clearAll());

    expect(calls).toEqual([{ ids: [1, 2] }]);
    expect(onCleared).toHaveBeenCalledTimes(1);
    expect(result.current.skipped).toBeNull();
  });

  it("keeps the error of a failed remove until it is dismissed", async () => {
    mockIPC(() => {
      throw { code: "ConversionRunning", detail: null };
    });
    const { result, onRemoved, onCleared } = setup(
      [{ id: 1 }],
      async () => null,
    );

    await act(() => result.current.remove(1));

    expect(onRemoved).not.toHaveBeenCalled();
    expect(result.current.error).toEqual({
      code: "ConversionRunning",
      detail: null,
    });

    await act(() => result.current.clearAll());
    expect(onCleared).not.toHaveBeenCalled();

    act(() => result.current.dismissError());
    expect(result.current.error).toBeNull();
  });

  it("drops the last error when the next add starts", async () => {
    const add = vi
      .fn<(source: AddSource) => Promise<AddResult<Item> | null>>()
      .mockRejectedValueOnce({ code: "ReadFailed", detail: null })
      .mockResolvedValueOnce(null);
    const { result } = setup([], add);

    await act(() => result.current.add("files"));
    expect(result.current.error?.code).toBe("ReadFailed");

    await act(() => result.current.add("files"));
    expect(result.current.error).toBeNull();
  });
});
