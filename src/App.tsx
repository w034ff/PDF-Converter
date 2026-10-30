import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type ReactNode,
} from "react";
import { ErrorDisplay, SelectBox } from "./components";
import { AboutDialog } from "./features/about/AboutDialog";
import { ImagesToPdfAction } from "./features/images-to-pdf/ImagesToPdfAction";
import { ImagesToPdfSettings } from "./features/images-to-pdf/ImagesToPdfSettings";
import { ImagesToPdfStatus } from "./features/images-to-pdf/ImagesToPdfStatus";
import { CropAction } from "./features/crop/CropAction";
import { CropScreen } from "./features/crop/CropScreen";
import { CropStatus } from "./features/crop/CropStatus";
import { useCropDropped } from "./features/crop/useCropDropped";
import { useCropAreaQueries } from "./features/crop/useCropAreaQueries";
import { useCropLayoutPage } from "./features/crop/useCropLayoutPage";
import { useCropPageData } from "./features/crop/useCropPageData";
import { useDropTarget } from "./features/crop/useDropTarget";
import { ImagesToPdfView } from "./features/images-to-pdf/ImagesToPdfView";
import { useItemsDropped } from "./features/items/useItemsDropped";
import { JobFooter } from "./features/job/JobFooter";
import { useJobEvents } from "./features/job/useJobEvents";
import { PdfToImagesAction } from "./features/pdf-to-images/PdfToImagesAction";
import { PdfToImagesSettings } from "./features/pdf-to-images/PdfToImagesSettings";
import { PdfToImagesStatus } from "./features/pdf-to-images/PdfToImagesStatus";
import { PdfToImagesView } from "./features/pdf-to-images/PdfToImagesView";
import { OrganizeAction } from "./features/organize/OrganizeAction";
import { OrganizeSettings } from "./features/organize/OrganizeSettings";
import { OrganizeStatus } from "./features/organize/OrganizeStatus";
import { OrganizeView } from "./features/organize/OrganizeView";
import { useOrganizeDropped } from "./features/organize/useOrganizeDropped";
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

/** The tabs in the order the header shows them. */
const TABS: readonly ActiveTab[] = [
  "imagesToPdf",
  "pdfToImages",
  "crop",
  "organize",
];

function AppShell() {
  const { language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  useJobEvents();
  useSettingsAutoSave();
  useBlockBrowserShortcuts();
  const dropError = useItemsDropped();
  useCropDropped();
  useOrganizeDropped();
  useDropTarget(language.activeTab);
  useCropPageData(language.activeTab === "crop");
  useCropAreaQueries();
  useCropLayoutPage();

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

  const activeTab = language.activeTab;
  const footerParts = {
    imagesToPdf: {
      idleStatus: <ImagesToPdfStatus />,
      action: <ImagesToPdfAction />,
    },
    pdfToImages: {
      idleStatus: <PdfToImagesStatus />,
      action: <PdfToImagesAction />,
    },
    crop: { idleStatus: <CropStatus />, action: <CropAction /> },
    organize: { idleStatus: <OrganizeStatus />, action: <OrganizeAction /> },
  } satisfies Record<ActiveTab, { idleStatus: ReactNode; action: ReactNode }>;

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
            {TABS.map((tab) => (
              <button
                key={tab}
                type="button"
                role="tab"
                aria-selected={activeTab === tab}
                className="app-nav-tab"
                onClick={() => handleTabChange(tab)}
              >
                {t.app.tabs[tab]}
              </button>
            ))}
          </nav>

          <div className="app-header-actions">
            <SelectBox>
              <select
                aria-label={t.app.languageLabel}
                className="lang-select"
                value={language.language}
                onChange={handleLanguageChange}
              >
                <option value="ja">日本語</option>
                <option value="en">English</option>
              </select>
            </SelectBox>
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
          {activeTab === "crop" ? (
            <CropScreen />
          ) : activeTab === "organize" ? (
            <>
              <aside className="app-sidebar organize-sidebar">
                <OrganizeSettings />
              </aside>

              <main className="app-main organize-main">
                <OrganizeView />
              </main>
            </>
          ) : (
            <>
              <aside className="app-sidebar">
                {activeTab === "imagesToPdf" ? (
                  <ImagesToPdfSettings />
                ) : (
                  <PdfToImagesSettings />
                )}
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
                {activeTab === "imagesToPdf" ? (
                  <ImagesToPdfView />
                ) : (
                  <PdfToImagesView />
                )}
              </main>
            </>
          )}
        </div>

        <footer className="app-footer">
          <JobFooter
            tab={activeTab}
            idleStatus={footerParts[activeTab].idleStatus}
            action={footerParts[activeTab].action}
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
