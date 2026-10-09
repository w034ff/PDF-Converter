import { useEffect, useRef, useState } from "react";
import appLicenseText from "../../../LICENSE?raw";
import { formatMessage, getTranslations } from "../../i18n";
import { getAbout, type AboutInfo } from "../../ipc/about";
import type { ThirdPartyLicense } from "../../licenses";
import { useAppState } from "../../state";

const DIALOG_TITLE_ID = "about-dialog-title";
const THIRD_PARTY_LIST_ID = "about-third-party-licenses";

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), details > summary';

type AboutLoad =
  | { state: "loading" }
  | { state: "loaded"; info: AboutInfo }
  | { state: "failed" };

export interface AboutDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

/**
 * "About this app" (FR-08): the version, whether pdfium loads, this app's
 * MIT license, and the third-party licenses (design §8.3).
 * The third-party list is loaded only when it is first expanded, since it is
 * far larger than the rest of the app's text.
 */
export function AboutDialog({ isOpen, onClose }: AboutDialogProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  const [about, setAbout] = useState<AboutLoad>({ state: "loading" });
  const [licenses, setLicenses] = useState<readonly ThirdPartyLicense[] | null>(
    null,
  );
  const [isExpanded, setIsExpanded] = useState(false);

  const dialogRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  const [wasOpen, setWasOpen] = useState(isOpen);
  if (wasOpen !== isOpen) {
    setWasOpen(isOpen);
    if (!isOpen) {
      setIsExpanded(false);
    }
  }

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    closeButtonRef.current?.focus();

    let isMounted = true;
    getAbout().then(
      (info) => {
        if (isMounted) {
          setAbout({ state: "loaded", info });
        }
      },
      () => {
        if (isMounted) {
          setAbout({ state: "failed" });
        }
      },
    );
    return () => {
      isMounted = false;
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      // The rest of the app is inert while the dialog is open, so Tab only
      // has to wrap around inside the dialog.
      const dialog = dialogRef.current;
      if (dialog === null) {
        return;
      }
      const focusable =
        dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      const outside = !dialog.contains(active);
      if (event.shiftKey && (active === first || outside)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || outside)) {
        event.preventDefault();
        first.focus();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [isOpen]);

  function handleToggleLicenses() {
    const next = !isExpanded;
    setIsExpanded(next);
    if (next && licenses === null) {
      import("../../licenses").then(
        (module) => setLicenses(module.thirdPartyLicenses),
        () => {
          // The list is bundled with the app; it fails to load only if the
          // build is broken, and then the dialog keeps showing "loading".
        },
      );
    }
  }

  if (!isOpen) {
    return null;
  }

  return (
    <div
      className="modal-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onCloseRef.current();
        }
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={DIALOG_TITLE_ID}
        className="modal-dialog"
      >
        <div className="modal-header">
          <h2 id={DIALOG_TITLE_ID} className="modal-title">
            {t.about.title}
          </h2>
          <button
            ref={closeButtonRef}
            type="button"
            className="icon-btn"
            aria-label={t.about.close}
            onClick={() => onCloseRef.current()}
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <div className="modal-body">
          <div className="about-app-info">
            <div className="about-app-name">{t.app.title}</div>
            {about.state === "loaded" ? (
              <>
                <div className="about-version">
                  {formatMessage(t.about.version, {
                    version: about.info.version,
                  })}
                </div>
                {!about.info.pdfiumReady && (
                  <div className="about-pdfium-error" role="alert">
                    {formatMessage(t.about.pdfiumUnavailable, {
                      error: about.info.pdfiumError ?? "",
                    })}
                  </div>
                )}
              </>
            ) : about.state === "failed" ? (
              <div className="about-version">{t.about.loadFailed}</div>
            ) : (
              // Holds the line so the dialog does not shift when it loads.
              <div className="about-version">{" "}</div>
            )}
          </div>

          <section className="about-section">
            <h3 className="about-section-title">{t.about.appLicense}</h3>
            <pre className="license-box">{appLicenseText}</pre>
          </section>

          <section className="about-section">
            <button
              type="button"
              className="btn btn-ghost about-toggle-btn"
              aria-expanded={isExpanded}
              aria-controls={isExpanded ? THIRD_PARTY_LIST_ID : undefined}
              onClick={handleToggleLicenses}
            >
              {isExpanded
                ? t.about.hideThirdPartyLicenses
                : t.about.showThirdPartyLicenses}
            </button>
            {isExpanded && (
              <div id={THIRD_PARTY_LIST_ID} className="about-licenses-list">
                {licenses === null ? (
                  <div className="hint">{t.about.loadingLicenses}</div>
                ) : (
                  licenses.map((license, licenseIndex) => (
                    <div
                      key={`${license.id}-${licenseIndex}`}
                      className="license-item"
                    >
                      <div className="license-item-header">{license.name}</div>
                      <ul className="license-packages">
                        {license.packages.map((pkg, pkgIndex) => (
                          <li
                            key={`${pkg.ecosystem}-${pkg.name}-${pkg.version}-${pkgIndex}`}
                          >
                            {pkg.name} {pkg.version}
                          </li>
                        ))}
                      </ul>
                      <details className="license-details">
                        <summary>{t.about.viewLicenseText}</summary>
                        <pre className="license-box">{license.text}</pre>
                      </details>
                    </div>
                  ))
                )}
              </div>
            )}
          </section>
        </div>

        <div className="modal-footer">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => onCloseRef.current()}
          >
            {t.about.close}
          </button>
        </div>
      </div>
    </div>
  );
}
