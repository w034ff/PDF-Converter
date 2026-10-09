import { ErrorDisplay } from "../../components";
import { formatErrorMessage } from "../../i18n";
import { useAppState, type ActiveTab } from "../../state";
import { isOutputDirError } from "./outputDirErrors";

export interface OutputDirErrorProps {
  /** The screen the band belongs to; it shows only that screen's error. */
  tab: ActiveTab;
}

/**
 * The band above a list that says a conversion did not start because its
 * output folder is gone or cannot be written to (design §6.5, §10.3). The
 * bottom bar does not repeat it.
 *
 * It has no close button: it stays while the folder is unusable, and goes
 * once a folder is chosen, a setting changes or a conversion starts.
 */
export function OutputDirError({ tab }: OutputDirErrorProps) {
  const { language, job } = useAppState();
  if (job.kind !== tab || job.error === null || !isOutputDirError(job.error)) {
    return null;
  }
  return (
    <ErrorDisplay
      message={formatErrorMessage(
        job.error.code,
        job.error.detail,
        language.language,
      )}
    />
  );
}
