import { detectInitialLanguage } from "../i18n";
import type { LanguageAction, LanguageState } from "./types";

export function createInitialLanguageState(
  initialLang?: string,
): LanguageState {
  return {
    language: detectInitialLanguage(initialLang),
    activeTab: "imagesToPdf",
  };
}

export function languageReducer(
  state: LanguageState,
  action: LanguageAction,
): LanguageState {
  switch (action.type) {
    case "SET_LANGUAGE":
      if (state.language === action.language) {
        return state;
      }
      return { ...state, language: action.language };
    case "SET_ACTIVE_TAB":
      if (state.activeTab === action.tab) {
        return state;
      }
      return { ...state, activeTab: action.tab };
  }
}
