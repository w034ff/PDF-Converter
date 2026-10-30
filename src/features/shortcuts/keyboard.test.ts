import { describe, expect, it } from "vitest";
import { historyKeyOf, isModalOpen, takesText } from "./keyboard";

const NONE = { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false };

describe("historyKeyOf", () => {
  it("is undo for Ctrl+Z, and redo for Ctrl+Y and Ctrl+Shift+Z", () => {
    expect(historyKeyOf({ ...NONE, key: "z", ctrlKey: true })).toBe("undo");
    expect(historyKeyOf({ ...NONE, key: "Z", ctrlKey: true })).toBe("undo");
    expect(historyKeyOf({ ...NONE, key: "y", ctrlKey: true })).toBe("redo");
    expect(
      historyKeyOf({ ...NONE, key: "Z", ctrlKey: true, shiftKey: true }),
    ).toBe("redo");
  });

  it("is nothing without Ctrl, or with Alt or Meta, or Shift on Y", () => {
    expect(historyKeyOf({ ...NONE, key: "z" })).toBeNull();
    expect(
      historyKeyOf({ ...NONE, key: "z", ctrlKey: true, altKey: true }),
    ).toBeNull();
    expect(
      historyKeyOf({ ...NONE, key: "z", ctrlKey: true, metaKey: true }),
    ).toBeNull();
    expect(
      historyKeyOf({ ...NONE, key: "y", ctrlKey: true, shiftKey: true }),
    ).toBeNull();
    expect(historyKeyOf({ ...NONE, key: "a", ctrlKey: true })).toBeNull();
  });
});

describe("takesText", () => {
  it("is true for a text field, a text area and editable content", () => {
    const text = document.createElement("input");
    text.type = "text";
    expect(takesText(text)).toBe(true);
    expect(takesText(document.createElement("textarea"))).toBe(true);
    const editable = document.createElement("div");
    Object.defineProperty(editable, "isContentEditable", { value: true });
    expect(takesText(editable)).toBe(true);
  });

  it("is false for the inputs that take no text, and for anything else", () => {
    for (const type of ["checkbox", "radio", "button", "submit", "reset"]) {
      const input = document.createElement("input");
      input.type = type;
      expect(takesText(input), type).toBe(false);
    }
    // jsdom does not know `isContentEditable`, so these are falsy there, false in a browser.
    expect(takesText(document.createElement("button"))).toBeFalsy();
    expect(takesText(null)).toBe(false);
  });
});

describe("isModalOpen", () => {
  it("is true while an element says it is a modal dialog", () => {
    expect(isModalOpen()).toBe(false);
    const dialog = document.createElement("div");
    dialog.setAttribute("aria-modal", "true");
    document.body.append(dialog);

    expect(isModalOpen()).toBe(true);
    dialog.remove();
    expect(isModalOpen()).toBe(false);
  });
});
