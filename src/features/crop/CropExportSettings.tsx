import { useId } from "react";
import { ChevronDownIcon } from "../../components";
import { getTranslations } from "../../i18n";
import {
  isJobActive,
  useAppDispatch,
  useAppState,
  type CropState,
} from "../../state";
import { OutputDirField } from "../output/OutputDirField";
import { RenderOptionFields } from "../pdf-to-images/RenderOptionFields";
import "./crop.css";

/** The settings on one line, for the button while the panel is closed (design §10.1). */
function summarize(
  crop: CropState,
  t: ReturnType<typeof getTranslations>,
): string {
  const parts = [
    t.pdfToImages.settings[
      crop.format === "png"
        ? "formatPng"
        : crop.format === "jpeg"
          ? "formatJpeg"
          : "formatWebp"
    ],
    crop.colorMode === "color"
      ? t.pdfToImages.settings.colorFullColor
      : t.pdfToImages.settings.colorGrayscale,
    `${crop.dpi} ${t.pdfToImages.settings.customDpiUnit}`,
  ];
  if (crop.photoNative) {
    parts.push(t.crop.settings.summaryPhotoNative);
  }
  if (crop.photoOnly) {
    parts.push(t.crop.settings.summaryPhotoOnly);
  }
  return parts.join(t.crop.settings.summarySeparator);
}

/**
 * The export settings under the list of areas (mockup `CropA`): a heading
 * that opens and closes them, and while they are closed the settings in one
 * line on the heading. The same fields as "PDF to images", plus what to do
 * with photos and the folder to save in.
 */
export function CropExportSettings() {
  const { language, crop, job } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const busy = isJobActive(job);
  const bodyId = useId();

  return (
    <div className="crop-settings">
      <button
        type="button"
        className="crop-settings-toggle"
        aria-expanded={crop.settingsOpen}
        aria-controls={bodyId}
        onClick={() =>
          dispatch({ type: "SET_CROP_SETTINGS_OPEN", open: !crop.settingsOpen })
        }
      >
        <span className="crop-settings-heading">
          <span className="label">{t.crop.settings.title}</span>
          <span
            className={`crop-settings-chevron${crop.settingsOpen ? " is-open" : ""}`}
          >
            <ChevronDownIcon />
          </span>
        </span>
        {!crop.settingsOpen && (
          <span className="hint crop-settings-summary">
            {summarize(crop, t)}
          </span>
        )}
      </button>
      {crop.settingsOpen && (
        <div id={bodyId} className="crop-settings-body">
          <RenderOptionFields
            format={crop.format}
            colorMode={crop.colorMode}
            dpi={crop.dpi}
            isCustomDpi={crop.isCustomDpi}
            customDpiText={crop.customDpiText}
            onFormatChange={(format) =>
              dispatch({ type: "SET_CROP_FORMAT", format })
            }
            onColorModeChange={(colorMode) =>
              dispatch({ type: "SET_CROP_COLOR_MODE", colorMode })
            }
            onDpiChange={(dpi) => dispatch({ type: "SET_CROP_DPI", dpi })}
            onCustomDpiSelected={() =>
              dispatch({ type: "SET_CROP_IS_CUSTOM_DPI", isCustom: true })
            }
            onCustomDpiTextChange={(text) =>
              dispatch({ type: "SET_CROP_CUSTOM_DPI_TEXT", text })
            }
            disabled={busy}
          />
          <div className="crop-settings-field">
            <label className="label crop-settings-checkbox">
              <input
                type="checkbox"
                checked={crop.photoNative}
                disabled={busy}
                onChange={(event) =>
                  dispatch({
                    type: "SET_CROP_PHOTO_NATIVE",
                    photoNative: event.target.checked,
                  })
                }
              />
              {t.crop.settings.photoNative}
            </label>
            <span className="hint crop-settings-hint">
              {t.crop.settings.photoNativeHint}
            </span>
          </div>
          <div className="crop-settings-field">
            <label className="label crop-settings-checkbox">
              <input
                type="checkbox"
                checked={crop.photoOnly}
                disabled={busy}
                onChange={(event) =>
                  dispatch({
                    type: "SET_CROP_PHOTO_ONLY",
                    photoOnly: event.target.checked,
                  })
                }
              />
              {t.crop.settings.photoOnly}
            </label>
            <span className="hint crop-settings-hint">
              {t.crop.settings.photoOnlyHint}
            </span>
          </div>
          <OutputDirField
            kind="crop"
            value={crop.outputDir}
            onChange={(outputDir) =>
              dispatch({ type: "SET_CROP_OUTPUT_DIR", outputDir })
            }
            disabled={busy}
          />
        </div>
      )}
    </div>
  );
}
