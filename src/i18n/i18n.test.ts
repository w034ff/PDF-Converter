import { describe, expect, it } from "vitest";
import type { ErrorCode } from "../ipc";
import {
  detectInitialLanguage,
  en,
  formatErrorMessage,
  ja,
  type Translations,
} from "./index";

const ALL_ERROR_CODES: ErrorCode[] = [
  "UnsupportedFormat",
  "DecodeFailed",
  "PdfOpenFailed",
  "PasswordProtected",
  "TooLarge",
  "TooManyPages",
  "RenderTooLarge",
  "WorkerCrashed",
  "WorkerTimeout",
  "ReadFailed",
  "WriteFailed",
  "InvalidPageRange",
  "ConversionRunning",
  "UnknownHandle",
  "InvalidParams",
];

describe("i18n", () => {
  describe("detectInitialLanguage", () => {
    it("returns ja when language starts with ja", () => {
      expect(detectInitialLanguage("ja-JP")).toBe("ja");
      expect(detectInitialLanguage("ja")).toBe("ja");
      expect(detectInitialLanguage("JA-jp")).toBe("ja");
    });

    it("returns en when language starts with en or other", () => {
      expect(detectInitialLanguage("en-US")).toBe("en");
      expect(detectInitialLanguage("en-GB")).toBe("en");
      expect(detectInitialLanguage("fr-FR")).toBe("en");
    });

    it("returns en when language is empty or undefined", () => {
      expect(detectInitialLanguage("")).toBe("en");
      expect(detectInitialLanguage(undefined)).toBe("en");
    });
  });

  describe("formatErrorMessage", () => {
    it("formats all 15 error codes in Japanese", () => {
      expect(formatErrorMessage("UnsupportedFormat", null, "ja")).toBe(
        "対応していない形式です",
      );
      expect(formatErrorMessage("DecodeFailed", null, "ja")).toBe(
        "画像を読み込めませんでした。ファイルが壊れている可能性があります",
      );
      expect(formatErrorMessage("PdfOpenFailed", null, "ja")).toBe(
        "PDF を読み込めませんでした。ファイルが壊れている可能性があります",
      );
      expect(formatErrorMessage("PasswordProtected", null, "ja")).toBe(
        "パスワードで保護されているため開けません",
      );
      expect(formatErrorMessage("TooLarge", "80,000,000", "ja")).toBe(
        "画像が大きすぎます（上限 80,000,000 ピクセル）",
      );
      expect(formatErrorMessage("TooManyPages", "10,000", "ja")).toBe(
        "ページ数が多すぎます（上限 10,000 ページ）",
      );
      expect(formatErrorMessage("RenderTooLarge", null, "ja")).toBe(
        "この解像度では大きすぎて画像にできません。解像度を下げてください",
      );
      expect(formatErrorMessage("WorkerCrashed", null, "ja")).toBe(
        "PDF の処理中に問題が起きました。ファイルが壊れている可能性があります",
      );
      expect(formatErrorMessage("WorkerTimeout", null, "ja")).toBe(
        "PDF の処理に時間がかかりすぎたため中止しました",
      );
      expect(formatErrorMessage("ReadFailed", null, "ja")).toBe(
        "ファイルの読み込みに失敗しました",
      );
      expect(formatErrorMessage("WriteFailed", null, "ja")).toBe(
        "ファイルの書き込みに失敗しました",
      );
      expect(formatErrorMessage("InvalidPageRange", "3-1", "ja")).toBe(
        "ページの範囲の書き方が正しくありません（3-1）",
      );
      expect(formatErrorMessage("ConversionRunning", null, "ja")).toBe(
        "変換中は操作できません",
      );
      expect(formatErrorMessage("UnknownHandle", null, "ja")).toBe(
        "ファイルをもう一度追加してください",
      );
      expect(formatErrorMessage("InvalidParams", null, "ja")).toBe(
        "無効な設定です",
      );
    });

    it("formats all 15 error codes in English", () => {
      expect(formatErrorMessage("UnsupportedFormat", null, "en")).toBe(
        "Unsupported file format",
      );
      expect(formatErrorMessage("DecodeFailed", null, "en")).toBe(
        "Couldn't read the image. The file may be damaged.",
      );
      expect(formatErrorMessage("PdfOpenFailed", null, "en")).toBe(
        "Couldn't read the PDF. The file may be damaged.",
      );
      expect(formatErrorMessage("PasswordProtected", null, "en")).toBe(
        "This PDF is password-protected",
      );
      expect(formatErrorMessage("TooLarge", "80,000,000", "en")).toBe(
        "Image is too large (limit: 80,000,000 pixels)",
      );
      expect(formatErrorMessage("TooManyPages", "10,000", "en")).toBe(
        "Too many pages (limit: 10,000)",
      );
      expect(formatErrorMessage("RenderTooLarge", null, "en")).toBe(
        "Too large at this resolution. Choose a lower resolution.",
      );
      expect(formatErrorMessage("WorkerCrashed", null, "en")).toBe(
        "Something went wrong while processing the PDF. The file may be damaged.",
      );
      expect(formatErrorMessage("WorkerTimeout", null, "en")).toBe(
        "Processing the PDF took too long and was stopped",
      );
      expect(formatErrorMessage("ReadFailed", null, "en")).toBe(
        "Failed to read file",
      );
      expect(formatErrorMessage("WriteFailed", null, "en")).toBe(
        "Failed to write file",
      );
      expect(formatErrorMessage("InvalidPageRange", "3-1", "en")).toBe(
        "Invalid page range (3-1)",
      );
      expect(formatErrorMessage("ConversionRunning", null, "en")).toBe(
        "Not available during conversion",
      );
      expect(formatErrorMessage("UnknownHandle", null, "en")).toBe(
        "Please add the file again",
      );
      expect(formatErrorMessage("InvalidParams", null, "en")).toBe(
        "Invalid settings",
      );
    });

    it("covers all error codes in design §6.6", () => {
      for (const code of ALL_ERROR_CODES) {
        expect(formatErrorMessage(code, "detail", "ja")).not.toBe("");
        expect(formatErrorMessage(code, "detail", "en")).not.toBe("");
      }
    });
  });

  describe("dictionary keys parity", () => {
    it("has identical keys structure between ja and en", () => {
      function isRecord(value: unknown): value is Record<string, unknown> {
        return typeof value === "object" && value !== null;
      }

      function getKeys(obj: Record<string, unknown>, prefix = ""): string[] {
        const keys: string[] = [];
        for (const k of Object.keys(obj)) {
          const val = obj[k];
          const fullKey = prefix ? `${prefix}.${k}` : k;
          if (isRecord(val)) {
            keys.push(...getKeys(val, fullKey));
          } else {
            keys.push(fullKey);
          }
        }
        return keys.sort();
      }

      const jaKeys = getKeys(ja);
      const enKeys = getKeys(en);
      expect(enKeys).toEqual(jaKeys);
    });

    it("satisfies Translations type for en dictionary", () => {
      // TypeScript compiler checks `const en: Translations` at build time.
      const typedEn: Translations = en;
      expect(typedEn).toBeDefined();
    });
  });
});
