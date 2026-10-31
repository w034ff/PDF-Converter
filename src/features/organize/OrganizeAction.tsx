import { useMemo, useRef, useState } from "react";
import { getTranslations } from "../../i18n";
import {
  isJobActive,
  MAX_PDF_PAGES,
  splitIntoParts,
  useAppDispatch,
  useAppState,
} from "../../state";
import { OutputPasswordDialog } from "../images-to-pdf/OutputPasswordDialog";
import { useJobRunner } from "../job/useJobRunner";
import { useEnsureOutputDir } from "../output";

/**
 * The bottom bar's button for the organize tab (organize §4). "PDF を保存"
 * saves the pages of the grid, in their order and as they are turned, as one
 * PDF. When the file saved was one of the PDFs of the list, that PDF has left
 * the list (its pages would be the old file's), and the screen takes it out as
 * well. "保存を開始" saves one PDF for each part between the breaks into the
 * output folder, asking for the folder first if there is none. Both ask for
 * the password first when the setting says so.
 */
export function OrganizeAction() {
  const { language, organize, job } = useAppState();
  const dispatch = useAppDispatch();
  const runner = useJobRunner();
  const { runWithOutputDir, isPicking } = useEnsureOutputDir("organize");
  const t = getTranslations(language.language);
  const [passwordOpen, setPasswordOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const { pages, pdfs, breaks } = organize;
  const isSplit = organize.output === "split";
  // The same parts the footer counts and the badges number.
  const parts = useMemo(
    () => (isSplit ? splitIntoParts(pages, breaks) : []),
    [isSplit, pages, breaks],
  );
  const disabled =
    pages.length === 0 ||
    (isSplit
      ? parts.some((part) => part.length > MAX_PDF_PAGES) || isPicking
      : pages.length > MAX_PDF_PAGES) ||
    isJobActive(job);

  async function start(password: string | null) {
    if (isSplit) {
      await runWithOutputDir(organize.outputDir, () =>
        runner.startOrganizeSplit(parts, password),
      );
      return;
    }
    // The list cannot change while the PDF is made, so this is the list the
    // answer is about.
    const replacedId = await runner.saveOrganizedPdf(pages, password);
    if (replacedId === null) {
      return;
    }
    const name = pdfs.find((pdf) => pdf.id === replacedId)?.name;
    dispatch({ type: "ORGANIZE_PDF_REPLACED", id: replacedId });
    if (name !== undefined) {
      dispatch({ type: "JOB_REPLACED", name });
    }
  }

  // The password goes from the dialog to the command and nowhere else (design §6.9).
  function handleConfirm(password: string) {
    setPasswordOpen(false);
    void start(password);
  }

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn btn-primary app-footer-action"
        disabled={disabled}
        onClick={() =>
          organize.encrypt ? setPasswordOpen(true) : void start(null)
        }
      >
        {isSplit ? t.footer.startSaving : t.footer.savePdf}
      </button>
      <OutputPasswordDialog
        isOpen={passwordOpen}
        mode={isSplit ? "each" : "merge"}
        count={isSplit ? parts.length : 1}
        startEachLabel={t.footer.startSaving}
        onCancel={() => setPasswordOpen(false)}
        onConfirm={handleConfirm}
        returnFocusRef={buttonRef}
      />
    </>
  );
}
