import { describe, expect, it } from "vitest";
import {
  hasOnlyAllowedCharacters,
  isOutputPasswordTooLong,
  OUTPUT_PASSWORD_MAX_LENGTH,
} from "./outputPassword";

describe("the password rule, as in OutputPassword::new", () => {
  it("accepts the printable ASCII characters and the space", () => {
    expect(hasOnlyAllowedCharacters(" ")).toBe(true);
    expect(hasOnlyAllowedCharacters("U15 pass-word!")).toBe(true);
    expect(hasOnlyAllowedCharacters("~")).toBe(true);
    const all = Array.from({ length: 0x7e - 0x20 + 1 }, (_, i) =>
      String.fromCharCode(0x20 + i),
    ).join("");
    expect(hasOnlyAllowedCharacters(all)).toBe(true);
  });

  it("refuses control characters, DEL and everything outside ASCII", () => {
    for (const password of [
      "a\u001fb",
      "a\u007fb",
      "a\tb",
      "ＡＢＣ１２３",
      "a　b",
      "パスワード",
      "ｱｲｳ",
      "é",
      "😀",
    ]) {
      expect(hasOnlyAllowedCharacters(password), password).toBe(false);
    }
  });

  it("counts 127 characters as the limit", () => {
    expect(OUTPUT_PASSWORD_MAX_LENGTH).toBe(127);
    expect(isOutputPasswordTooLong("a".repeat(127))).toBe(false);
    expect(isOutputPasswordTooLong("a".repeat(128))).toBe(true);
  });

  it("counts a character outside the BMP as one", () => {
    expect(isOutputPasswordTooLong("😀".repeat(127))).toBe(false);
    expect(isOutputPasswordTooLong("😀".repeat(128))).toBe(true);
  });
});
