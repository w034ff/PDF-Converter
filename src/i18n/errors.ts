import type { ErrorCode } from "../ipc";
import type { Language } from "./types";

export const ERROR_MESSAGES_JA: Record<ErrorCode, string> = {
  UnsupportedFormat: "対応していない形式です",
  DecodeFailed:
    "画像を読み込めませんでした。ファイルが壊れている可能性があります",
  PdfOpenFailed:
    "PDF を読み込めませんでした。ファイルが壊れている可能性があります",
  PasswordProtected: "パスワードで保護されているため開けません",
  TooLarge: "画像が大きすぎます（上限 {detail} ピクセル）",
  TooManyPages: "ページ数が多すぎます（上限 {detail} ページ）",
  RenderTooLarge:
    "この解像度では大きすぎて画像にできません。解像度を下げてください",
  WorkerCrashed:
    "PDF の処理中に問題が起きました。ファイルが壊れている可能性があります",
  WorkerTimeout: "PDF の処理に時間がかかりすぎたため中止しました",
  ReadFailed: "ファイルの読み込みに失敗しました",
  WriteFailed: "ファイルの書き込みに失敗しました",
  InvalidPageRange: "ページの範囲の書き方が正しくありません（{detail}）",
  ConversionRunning: "変換中は操作できません",
  UnknownHandle: "ファイルをもう一度追加してください",
  InvalidParams: "無効な設定です",
  OutputDirMissing:
    "保存先のフォルダが見つかりません。フォルダを選び直してください",
  OutputDirNotWritable: "保存先のフォルダに書き込めません",
};

export const ERROR_MESSAGES_EN: Record<ErrorCode, string> = {
  UnsupportedFormat: "Unsupported file format",
  DecodeFailed: "Couldn't read the image. The file may be damaged.",
  PdfOpenFailed: "Couldn't read the PDF. The file may be damaged.",
  PasswordProtected: "This PDF is password-protected",
  TooLarge: "Image is too large (limit: {detail} pixels)",
  TooManyPages: "Too many pages (limit: {detail})",
  RenderTooLarge: "Too large at this resolution. Choose a lower resolution.",
  WorkerCrashed:
    "Something went wrong while processing the PDF. The file may be damaged.",
  WorkerTimeout: "Processing the PDF took too long and was stopped",
  ReadFailed: "Failed to read file",
  WriteFailed: "Failed to write file",
  InvalidPageRange: "Invalid page range ({detail})",
  ConversionRunning: "Not available during conversion",
  UnknownHandle: "Please add the file again",
  InvalidParams: "Invalid settings",
  OutputDirMissing: "The output folder can't be found. Choose a folder again.",
  OutputDirNotWritable: "Can't write to the output folder",
};

/**
 * Returns the localized error message for an ErrorCode, replacing `{detail}`
 * with the provided detail string if present (design §6.6).
 */
export function formatErrorMessage(
  code: ErrorCode,
  detail: string | null | undefined,
  lang: Language,
): string {
  const table = lang === "ja" ? ERROR_MESSAGES_JA : ERROR_MESSAGES_EN;
  const template = table[code];
  if (!template) {
    return "";
  }
  if (
    detail !== null &&
    detail !== undefined &&
    template.includes("{detail}")
  ) {
    return template.replaceAll("{detail}", detail);
  }
  return template;
}
