import { describe, expect, it } from "vitest";
import { isBlockedBrowserShortcut } from "./blockedShortcuts";

function press(key: string, modifiers: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent("keydown", { key, cancelable: true, ...modifiers });
}

describe("isBlockedBrowserShortcut", () => {
  it.each([
    ["Ctrl+F", press("f", { ctrlKey: true })],
    ["F3", press("F3")],
    ["Ctrl+G", press("g", { ctrlKey: true })],
    ["Ctrl+Shift+G", press("G", { ctrlKey: true, shiftKey: true })],
    ["Ctrl+P", press("p", { ctrlKey: true })],
    ["Ctrl+R", press("r", { ctrlKey: true })],
    ["Ctrl+Shift+R", press("R", { ctrlKey: true, shiftKey: true })],
    ["F5", press("F5")],
    ["Ctrl+F5", press("F5", { ctrlKey: true })],
    ["Ctrl+Shift+P", press("P", { ctrlKey: true, shiftKey: true })],
    ["Ctrl+J", press("j", { ctrlKey: true })],
    ["F7", press("F7")],
  ])("blocks %s", (_name, event) => {
    expect(isBlockedBrowserShortcut(event)).toBe(true);
  });

  it("treats Command like Ctrl", () => {
    expect(isBlockedBrowserShortcut(press("f", { metaKey: true }))).toBe(true);
    expect(isBlockedBrowserShortcut(press("p", { metaKey: true }))).toBe(true);
  });

  it("ignores the case of the key", () => {
    expect(isBlockedBrowserShortcut(press("F", { ctrlKey: true }))).toBe(true);
  });

  it.each([
    ["Ctrl+C", press("c", { ctrlKey: true })],
    ["Ctrl+V", press("v", { ctrlKey: true })],
    ["Ctrl+X", press("x", { ctrlKey: true })],
    ["Ctrl+A", press("a", { ctrlKey: true })],
    ["Ctrl+Z", press("z", { ctrlKey: true })],
    ["Ctrl+Y", press("y", { ctrlKey: true })],
    ["Ctrl++", press("+", { ctrlKey: true, shiftKey: true })],
    ["Ctrl+=", press("=", { ctrlKey: true })],
    ["Ctrl+-", press("-", { ctrlKey: true })],
    ["Ctrl+0", press("0", { ctrlKey: true })],
    ["f typed", press("f")],
    ["r typed", press("r")],
    ["p typed", press("p")],
    ["j typed", press("j")],
    ["Ctrl+S", press("s", { ctrlKey: true })],
    ["Ctrl+U", press("u", { ctrlKey: true })],
    ["Shift+F7", press("F7", { shiftKey: true })],
    ["Tab", press("Tab")],
    ["F12", press("F12")],
    ["Ctrl+Alt+F (AltGr)", press("f", { ctrlKey: true, altKey: true })],
    ["Shift+F5", press("F5", { shiftKey: true })],
  ])("lets %s through", (_name, event) => {
    expect(isBlockedBrowserShortcut(event)).toBe(false);
  });
});
