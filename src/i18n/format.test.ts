import { describe, expect, it } from "vitest";
import { formatMessage, pluralize } from "./format";

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

describe("pluralize", () => {
  it("takes the template for one only for a count of one", () => {
    expect(pluralize(1, "{count} page", "{count} pages")).toBe("1 page");
    expect(pluralize(0, "{count} page", "{count} pages")).toBe("0 pages");
    expect(pluralize(2, "{count} page", "{count} pages")).toBe("2 pages");
  });
});
