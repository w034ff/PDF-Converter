import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useIpcEvent } from "./useIpcEvent";

/** A subscription whose `listen` resolves when the test says so. */
function fakeSubscription() {
  const stop = vi.fn();
  let emit: (payload: number) => void = () => {};
  let resolve: () => void = () => {};
  const subscribe = vi.fn(
    (handler: (payload: number) => void) =>
      new Promise<() => void>((done) => {
        emit = handler;
        resolve = () => done(stop);
      }),
  );
  return {
    stop,
    subscribe,
    emit: (payload: number) => emit(payload),
    resolve: () => act(async () => resolve()),
  };
}

describe("useIpcEvent", () => {
  it("stops listening on unmount", async () => {
    const event = fakeSubscription();
    const { unmount } = renderHook(() =>
      useIpcEvent(event.subscribe, () => {}),
    );
    await event.resolve();
    expect(event.stop).not.toHaveBeenCalled();

    unmount();

    expect(event.stop).toHaveBeenCalledTimes(1);
  });

  it("stops a subscription that resolves after the unmount", async () => {
    const event = fakeSubscription();
    const { unmount } = renderHook(() =>
      useIpcEvent(event.subscribe, () => {}),
    );

    unmount();
    expect(event.stop).not.toHaveBeenCalled();
    await event.resolve();

    expect(event.stop).toHaveBeenCalledTimes(1);
  });

  it("subscribes once and calls the latest handler", async () => {
    const event = fakeSubscription();
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(
      ({ handler }) => useIpcEvent(event.subscribe, handler),
      { initialProps: { handler: first } },
    );
    await event.resolve();

    rerender({ handler: second });
    event.emit(7);

    expect(event.subscribe).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith(7);
  });
});
