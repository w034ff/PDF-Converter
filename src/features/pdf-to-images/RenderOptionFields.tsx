import { useId, type ChangeEvent, type ReactNode } from "react";
import {
  SegmentedControl,
  SelectBox,
  type SegmentedOption,
} from "../../components";
import { formatMessage, getTranslations, type Translations } from "../../i18n";
import type { RenderColorChoice, RenderFormatChoice } from "../../ipc";
import {
  DPI_CHOICES,
  RENDER_DPI_MAX,
  RENDER_DPI_MIN,
  parseCustomDpi,
  useAppState,
} from "../../state";
import { useDelayedReveal } from "./useDelayedReveal";
import "./pdfToImages.css";

const DPI_LABEL_KEYS: Record<
  number,
  keyof Translations["pdfToImages"]["settings"]["dpiChoices"]
> = {
  72: "dpi72",
  150: "dpi150",
  300: "dpi300",
};

export interface RenderOptionFieldsProps {
  format: RenderFormatChoice;
  colorMode: RenderColorChoice;
  dpi: number;
  isCustomDpi: boolean;
  customDpiText: string;
  onFormatChange: (format: RenderFormatChoice) => void;
  onColorModeChange: (colorMode: RenderColorChoice) => void;
  /** A preset of the dropdown was picked. */
  onDpiChange: (dpi: number) => void;
  /** "数値で指定…" was picked. */
  onCustomDpiSelected: () => void;
  onCustomDpiTextChange: (text: string) => void;
  disabled?: boolean;
  /** Shown under the resolution field while its value is valid. */
  dpiNote?: ReactNode;
}

/**
 * The format, color and resolution fields that "PDF to images" and the crop
 * tab share (design §10.1), as the groups of a settings panel. The caller
 * lays them out.
 */
export function RenderOptionFields({
  format,
  colorMode,
  dpi,
  isCustomDpi,
  customDpiText,
  onFormatChange,
  onColorModeChange,
  onDpiChange,
  onCustomDpiSelected,
  onCustomDpiTextChange,
  disabled = false,
  dpiNote,
}: RenderOptionFieldsProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  const dpiSelectId = useId();

  // A custom value that is empty, malformed or out of range is flagged once
  // typing pauses or the field loses focus (design §10.1).
  const isCustomDpiError =
    isCustomDpi && parseCustomDpi(customDpiText) === null;
  const [isCustomDpiErrorShown, revealCustomDpiError] = useDelayedReveal(
    customDpiText,
    isCustomDpiError,
  );

  const formatOptions: readonly SegmentedOption<RenderFormatChoice>[] = [
    { value: "png", label: t.pdfToImages.settings.formatPng },
    { value: "jpeg", label: t.pdfToImages.settings.formatJpeg },
    { value: "webp", label: t.pdfToImages.settings.formatWebp },
  ];

  const colorOptions: readonly SegmentedOption<RenderColorChoice>[] = [
    { value: "color", label: t.pdfToImages.settings.colorFullColor },
    { value: "gray", label: t.pdfToImages.settings.colorGrayscale },
  ];

  function handleDpiSelectChange(e: ChangeEvent<HTMLSelectElement>) {
    if (e.target.value === "custom") {
      onCustomDpiSelected();
    } else {
      onDpiChange(Number(e.target.value));
    }
  }

  return (
    <>
      {/* 形式 */}
      <div className="pdf-settings-group">
        <span className="label">{t.pdfToImages.settings.formatTitle}</span>
        <SegmentedControl<RenderFormatChoice>
          label={t.pdfToImages.settings.formatTitle}
          options={formatOptions}
          value={format}
          onChange={onFormatChange}
          disabled={disabled}
        />
      </div>

      {/* 色 */}
      <div className="pdf-settings-group">
        <span className="label">{t.pdfToImages.settings.colorTitle}</span>
        <SegmentedControl<RenderColorChoice>
          label={t.pdfToImages.settings.colorTitle}
          options={colorOptions}
          value={colorMode}
          onChange={onColorModeChange}
          disabled={disabled}
        />
      </div>

      {/* 解像度 */}
      <div className="pdf-settings-group">
        <label className="label" htmlFor={dpiSelectId}>
          {t.pdfToImages.settings.dpiLabel}
        </label>
        <SelectBox>
          <select
            id={dpiSelectId}
            className="settings-control"
            value={isCustomDpi ? "custom" : String(dpi)}
            onChange={handleDpiSelectChange}
            disabled={disabled}
          >
            {DPI_CHOICES.map((choice) => {
              const key = DPI_LABEL_KEYS[choice];
              const label = key
                ? t.pdfToImages.settings.dpiChoices[key]
                : `${choice} dpi`;
              return (
                <option key={choice} value={choice}>
                  {label}
                </option>
              );
            })}
            <option value="custom">
              {t.pdfToImages.settings.dpiChoices.custom}
            </option>
          </select>
        </SelectBox>
        {isCustomDpi && (
          // The box is drawn by the label so that the unit sits inside it and
          // a click on the unit focuses the input.
          <label className="settings-control pdf-custom-dpi-field">
            <input
              type="text"
              inputMode="numeric"
              className="pdf-custom-dpi-input"
              aria-label={t.pdfToImages.settings.customDpiLabel}
              value={customDpiText}
              onChange={(e) => onCustomDpiTextChange(e.target.value)}
              onBlur={revealCustomDpiError}
              disabled={disabled}
            />
            <span className="pdf-custom-dpi-unit">
              {t.pdfToImages.settings.customDpiUnit}
            </span>
          </label>
        )}
        {isCustomDpiErrorShown && (
          <span className="error-text" role="alert">
            {formatMessage(t.pdfToImages.settings.customDpiError, {
              min: RENDER_DPI_MIN,
              max: RENDER_DPI_MAX,
            })}
          </span>
        )}
        {dpiNote}
      </div>
    </>
  );
}
