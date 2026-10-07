import { detectInitialLanguage, type Language } from "../i18n";
import type { LanguageAction, LanguageState } from "./types";

/**
 * Starts in the saved language, or in the OS language (`initialLang`, else
 * `navigator.language`) when none was saved.
 */
export function createInitialLanguageState(
  initialLang?: string,
  preference: Language | null = null,
): LanguageState {
  return {
    language: preference ?? detectInitialLanguage(initialLang),
    preference,
    activeTab: "imagesToPdf",
  };
}

export function languageReducer(
  state: LanguageState,
  action: LanguageAction,
): LanguageState {
  switch (action.type) {
    case "SET_LANGUAGE":
      if (
        state.language === action.language &&
        state.preference === action.language
      ) {
        return state;
      }
      return {
        ...state,
        language: action.language,
        preference: action.language,
      };
    case "SET_ACTIVE_TAB":
      if (state.activeTab === action.tab) {
        return state;
      }
      return { ...state, activeTab: action.tab };
  }
}
