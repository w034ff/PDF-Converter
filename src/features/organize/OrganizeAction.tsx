import { useRef, useState } from "react";
import { getTranslations } from "../../i18n";
import {
  isJobActive,
  MAX_PDF_PAGES,
  useAppDispatch,
  useAppState,
} from "../../state";
import { OutputPasswordDialog } from "../images-to-pdf/OutputPasswordDialog";
import { useJobRunner } from "../job/useJobRunner";

/**
 * The bottom bar's "PDF を保存" for the organize tab (organize §4): saves the
 * pages of the grid, in their order and as they are turned, as one PDF. It
 * asks for the password first when the setting says so. When the file saved
 * was one of the PDFs of the list, that PDF has left the list (its pages
 * would be the old file's), and the screen takes it out as well.
 */
export function OrganizeAction() {
  const { language, organize, job } = useAppState();
  const dispatch = useAppDispatch();
  const runner = useJobRunner();
  const t = getTranslations(language.language);
  const [passwordOpen, setPasswordOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const { pages, pdfs } = organize;
  const disabled =
    pages.length === 0 || pages.length > MAX_PDF_PAGES || isJobActive(job);

  async function start(password: string | null) {
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
        {t.footer.savePdf}
      </button>
      <OutputPasswordDialog
        isOpen={passwordOpen}
        mode="merge"
        count={1}
        onCancel={() => setPasswordOpen(false)}
        onConfirm={handleConfirm}
        returnFocusRef={buttonRef}
      />
    </>
  );
}
