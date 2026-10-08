import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { ErrorDisplay } from "./components";
import { AboutDialog } from "./features/about/AboutDialog";
import { ImagesToPdfAction } from "./features/images-to-pdf/ImagesToPdfAction";
import { ImagesToPdfSettings } from "./features/images-to-pdf/ImagesToPdfSettings";
import { ImagesToPdfStatus } from "./features/images-to-pdf/ImagesToPdfStatus";
import { ImagesToPdfView } from "./features/images-to-pdf/ImagesToPdfView";
import { useItemsDropped } from "./features/items/useItemsDropped";
import { JobFooter } from "./features/job/JobFooter";
import { useJobEvents } from "./features/job/useJobEvents";
import { PdfToImagesAction } from "./features/pdf-to-images/PdfToImagesAction";
import { PdfToImagesSettings } from "./features/pdf-to-images/PdfToImagesSettings";
import { PdfToImagesStatus } from "./features/pdf-to-images/PdfToImagesStatus";
import { PdfToImagesView } from "./features/pdf-to-images/PdfToImagesView";
import { useSettingsAutoSave } from "./features/settings/useSettingsAutoSave";
import { useBlockBrowserShortcuts } from "./features/shortcuts/useBlockBrowserShortcuts";
import { formatErrorMessage, getTranslations, type Language } from "./i18n";
import { getSettings, type Settings } from "./ipc";
import {
  AppStateProvider,
  createInitialAppState,
  useAppDispatch,
  useAppState,
  type ActiveTab,
} from "./state";
import "./styles/app.css";

function AppShell() {
  const { language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  useJobEvents();
  useSettingsAutoSave();
  useBlockBrowserShortcuts();
  const dropError = useItemsDropped();

  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const aboutButtonRef = useRef<HTMLButtonElement>(null);
  const wasAboutOpenRef = useRef(false);

  // Give focus back to the button that opened the dialog.
  useEffect(() => {
    if (wasAboutOpenRef.current && !isAboutOpen) {
      aboutButtonRef.current?.focus();
    }
    wasAboutOpenRef.current = isAboutOpen;
  }, [isAboutOpen]);

  function handleTabChange(tab: ActiveTab) {
    dispatch({ type: "SET_ACTIVE_TAB", tab });
  }

  function handleLanguageChange(e: ChangeEvent<HTMLSelectElement>) {
    const nextLang: Language = e.target.value === "en" ? "en" : "ja";
    dispatch({ type: "SET_LANGUAGE", language: nextLang });
  }

  const isImages = language.activeTab === "imagesToPdf";

  return (
    <>
      <div className="app-container" inert={isAboutOpen ? true : undefined}>
        <header className="app-header">
          <h1 className="app-title">
            <svg
              className="app-title-icon"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
              <path d="M14 3v6h6" />
            </svg>
            {t.app.title}
          </h1>

          <nav className="app-nav" aria-label={t.app.navAria} role="tablist">
            <button
              type="button"
              role="tab"
              aria-selected={isImages}
              className="app-nav-tab"
              onClick={() => handleTabChange("imagesToPdf")}
            >
              {t.app.tabs.imagesToPdf}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={!isImages}
              className="app-nav-tab"
              onClick={() => handleTabChange("pdfToImages")}
            >
              {t.app.tabs.pdfToImages}
            </button>
          </nav>

          <div className="app-header-actions">
            <select
              aria-label={t.app.languageLabel}
              className="lang-select"
              value={language.language}
              onChange={handleLanguageChange}
            >
              <option value="ja">日本語</option>
              <option value="en">English</option>
            </select>
            <button
              ref={aboutButtonRef}
              type="button"
              className="icon-btn"
              aria-label={t.app.aboutButtonAria}
              onClick={() => setIsAboutOpen(true)}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                aria-hidden="true"
              >
                <circle cx="12" cy="12" r="9" />
                <path d="M12 11v5" />
                <path d="M12 8h.01" />
              </svg>
            </button>
          </div>
        </header>

        <div className="app-body">
          <aside className="app-sidebar">
            {isImages ? <ImagesToPdfSettings /> : <PdfToImagesSettings />}
          </aside>

          <main className="app-main">
            {dropError.error !== null && (
              <ErrorDisplay
                message={formatErrorMessage(
                  dropError.error.code,
                  dropError.error.detail,
                  language.language,
                )}
                onDismiss={dropError.dismiss}
                dismissLabel={t.errors.dismiss}
              />
            )}
            {isImages ? <ImagesToPdfView /> : <PdfToImagesView />}
          </main>
        </div>

        <footer className="app-footer">
          <JobFooter
            tab={language.activeTab}
            idleStatus={
              isImages ? <ImagesToPdfStatus /> : <PdfToImagesStatus />
            }
            action={isImages ? <ImagesToPdfAction /> : <PdfToImagesAction />}
          />
        </footer>
      </div>

      <AboutDialog isOpen={isAboutOpen} onClose={() => setIsAboutOpen(false)} />
    </>
  );
}

export interface AppProps {
  /** The OS language to assume instead of `navigator.language`. */
  initialNavLang?: string;
  /**
   * Settings to start from instead of asking Rust with `get_settings`;
   * `null` starts from the defaults.
   */
  initialSettings?: Settings | null;
}

/**
 * The app. It shows nothing until the saved settings are read (design §6.7),
 * so the screen does not first appear in the defaults and then change.
 */
export function App({ initialNavLang, initialSettings }: AppProps) {
  const [settings, setSettings] = useState<
    { loaded: false } | { loaded: true; value: Settings | null }
  >(
    initialSettings === undefined
      ? { loaded: false }
      : { loaded: true, value: initialSettings },
  );

  useEffect(() => {
    if (settings.loaded) {
      return;
    }
    let isMounted = true;
    // Unreadable settings are not worth stopping the app for: Rust already
    // falls back to the defaults for a broken file, so a failure here means
    // the command itself failed, and the defaults are the best left.
    getSettings().then(
      (value) => {
        if (isMounted) {
          setSettings({ loaded: true, value });
        }
      },
      () => {
        if (isMounted) {
          setSettings({ loaded: true, value: null });
        }
      },
    );
    return () => {
      isMounted = false;
    };
  }, [settings.loaded]);

  if (!settings.loaded) {
    return <div className="app-container" />;
  }

  return (
    <AppStateProvider
      initialState={createInitialAppState(initialNavLang, settings.value)}
    >
      <AppShell />
    </AppStateProvider>
  );
}
