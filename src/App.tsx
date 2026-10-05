import { useEffect, useState, type ChangeEvent } from "react";
import { AboutDialog } from "./features/about/AboutDialog";
import { ImagesToPdfSettings } from "./features/images-to-pdf/ImagesToPdfSettings";
import { ImagesToPdfView } from "./features/images-to-pdf/ImagesToPdfView";
import { PdfToImagesSettings } from "./features/pdf-to-images/PdfToImagesSettings";
import { PdfToImagesView } from "./features/pdf-to-images/PdfToImagesView";
import { getTranslations, type Language } from "./i18n";
import { getAbout, type AboutInfo } from "./ipc/about";
import {
  AppStateProvider,
  createInitialAppState,
  useAppDispatch,
  useAppState,
  type ActiveTab,
} from "./state";
import "./styles/app.css";

interface AppShellProps {
  initialAbout?: AboutInfo | null;
}

function AppShell({ initialAbout = null }: AppShellProps) {
  const { language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);

  // Preserve T01 pdfium status verification for installer check until T13
  const [about, setAbout] = useState<AboutInfo | null>(initialAbout);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    getAbout().then(setAbout, (e: unknown) => setFailure(String(e)));
  }, []);

  function handleTabChange(tab: ActiveTab) {
    dispatch({ type: "SET_ACTIVE_TAB", tab });
  }

  function handleLanguageChange(e: ChangeEvent<HTMLSelectElement>) {
    const nextLang: Language = e.target.value === "en" ? "en" : "ja";
    dispatch({ type: "SET_LANGUAGE", language: nextLang });
  }

  const isImages = language.activeTab === "imagesToPdf";

  return (
    <div className="app-container">
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
            aria-pressed={isImages}
            className="app-nav-tab"
            onClick={() => handleTabChange("imagesToPdf")}
          >
            {t.app.tabs.imagesToPdf}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={!isImages}
            aria-pressed={!isImages}
            className="app-nav-tab"
            onClick={() => handleTabChange("pdfToImages")}
          >
            {t.app.tabs.pdfToImages}
          </button>
        </nav>

        <div className="app-header-actions">
          <label className="hint" htmlFor="lang">
            {t.app.languageLabel}
          </label>
          <select
            id="lang"
            className="lang-select"
            value={language.language}
            onChange={handleLanguageChange}
          >
            <option value="ja">日本語</option>
            <option value="en">English</option>
          </select>
          <button
            type="button"
            className="btn btn-ghost"
            aria-label={t.app.aboutButtonAria}
            style={{ width: "36px", padding: 0 }}
          >
            i
          </button>
        </div>
      </header>

      <div className="app-body">
        <aside className="app-sidebar">
          {isImages ? <ImagesToPdfSettings /> : <PdfToImagesSettings />}
        </aside>

        <main className="app-main">
          {isImages ? <ImagesToPdfView /> : <PdfToImagesView />}
        </main>
      </div>

      <footer className="app-footer">
        <span className="hint" data-testid="pdfium-status">
          {about
            ? about.pdfiumReady
              ? `pdfium ${about.pdfiumVersion}: OK (app ${about.version})`
              : `pdfium ${about.pdfiumVersion}: ${about.pdfiumError ?? ""}`
            : failure
              ? failure
              : isImages
                ? t.footer.noImagesSelected
                : t.footer.noPdfsSelected}
        </span>
        <button
          type="button"
          className="btn btn-primary"
          disabled
          style={{ marginLeft: "auto" }}
        >
          {isImages ? t.footer.savePdf : t.footer.startConversion}
        </button>
      </footer>

      <AboutDialog />
    </div>
  );
}

export interface AppProps {
  initialNavLang?: string;
  initialAbout?: AboutInfo | null;
}

export function App({ initialNavLang, initialAbout }: AppProps) {
  return (
    <AppStateProvider
      initialState={
        initialNavLang ? createInitialAppState(initialNavLang) : undefined
      }
    >
      <AppShell initialAbout={initialAbout} />
    </AppStateProvider>
  );
}
