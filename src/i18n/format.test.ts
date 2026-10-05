import { describe, expect, it } from "vitest";
import { formatMessage } from "./format";

describe("formatMessage", () => {
  it("fills every placeholder, including repeated ones", () => {
    expect(
      formatMessage("{name}: {count} / {count}", { name: "a.png", count: 3 }),
    ).toBe("a.png: 3 / 3");
  });

  it("leaves a placeholder without a value as it is", () => {
    expect(formatMessage("{name} ほか {count} 件", { name: "a.png" })).toBe(
      "a.png ほか {count} 件",
    );
  });

  it("does not read inherited properties as values", () => {
    expect(formatMessage("{toString}", {})).toBe("{toString}");
  });

  it("does not expand placeholders inside a value", () => {
    expect(formatMessage("{a}", { a: "{b}", b: "x" })).toBe("{b}");
  });
});
