import { describe, expect, it } from "vitest";
import { formatByteSize, formatMessage, pluralize } from "./format";

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

describe("formatByteSize", () => {
  it("shows whole bytes and KB, and MB to one decimal place", () => {
    expect(formatByteSize(0)).toBe("0 B");
    expect(formatByteSize(1023)).toBe("1023 B");
    expect(formatByteSize(1024)).toBe("1 KB");
    expect(formatByteSize(9500)).toBe("9 KB");
    expect(formatByteSize(9800)).toBe("10 KB");
    expect(formatByteSize(1258291)).toBe("1.2 MB");
    expect(formatByteSize(1992294)).toBe("1.9 MB");
    expect(formatByteSize(50 * 1024 * 1024)).toBe("50.0 MB");
  });

  it("goes over to MB where KB would round to 1,024", () => {
    expect(formatByteSize(1024 * 1024 - 600)).toBe("1023 KB");
    expect(formatByteSize(1024 * 1024 - 100)).toBe("1.0 MB");
  });
});
