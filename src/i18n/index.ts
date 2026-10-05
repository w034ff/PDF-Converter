import { en } from "./en";
import { ja, type Translations } from "./ja";
import type { Language } from "./types";

export * from "./errors";
export * from "./format";
export * from "./types";
export { en, ja, type Translations };

/**
 * Detects initial language according to design §10.2:
 * If navigator.language starts with "ja", use Japanese; otherwise use English.
 */
export function detectInitialLanguage(navLang?: string): Language {
  const languageString =
    navLang ?? (typeof navigator !== "undefined" ? navigator.language : "");
  if (languageString.toLowerCase().startsWith("ja")) {
    return "ja";
  }
  return "en";
}

/**
 * Returns dictionary translations for given language.
 */
export function getTranslations(lang: Language): Translations {
  return lang === "ja" ? ja : en;
}
