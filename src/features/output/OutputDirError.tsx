import { ErrorDisplay } from "../../components";
import { formatErrorMessage, getTranslations } from "../../i18n";
import { useAppDispatch, useAppState, type ActiveTab } from "../../state";
import { isOutputDirError } from "./outputDirErrors";

export interface OutputDirErrorProps {
  /** The screen the band belongs to; it shows only that screen's error. */
  tab: ActiveTab;
}

/**
 * The band above a list that says a conversion did not start because its
 * output folder is gone or cannot be written to (design §6.5, §10.3). The
 * bottom bar does not repeat it.
 */
export function OutputDirError({ tab }: OutputDirErrorProps) {
  const { language, job } = useAppState();
  const dispatch = useAppDispatch();
  if (job.kind !== tab || job.error === null || !isOutputDirError(job.error)) {
    return null;
  }
  const t = getTranslations(language.language);
  return (
    <ErrorDisplay
      message={formatErrorMessage(
        job.error.code,
        job.error.detail,
        language.language,
      )}
      onDismiss={() => dispatch({ type: "JOB_RESET" })}
      dismissLabel={t.errors.dismiss}
    />
  );
}
