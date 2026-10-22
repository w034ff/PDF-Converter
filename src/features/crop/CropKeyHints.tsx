import { useRef, type RefObject } from "react";
import { ModalDialog } from "../../components";
import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

export interface CropKeyHintsProps {
  onClose: () => void;
  /** Where the focus goes back to once the list is closed. */
  returnFocusRef: RefObject<HTMLElement | null>;
}

/**
 * The keys of the crop tab, listed in a dialog (design §10.1): none of them
 * show on the screen otherwise.
 */
export function CropKeyHints({ onClose, returnFocusRef }: CropKeyHintsProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  const closeRef = useRef<HTMLButtonElement>(null);

  return (
    <ModalDialog
      isOpen={true}
      onClose={onClose}
      titleId="crop-key-hints-title"
      dialogClassName="modal-dialog crop-key-hints"
      initialFocusRef={closeRef}
      returnFocusRef={returnFocusRef}
    >
      <h2 id="crop-key-hints-title" className="modal-title">
        {t.crop.keyHints.title}
      </h2>
      <table className="crop-key-hints-table">
        <thead>
          <tr>
            <th scope="col">{t.crop.keyHints.keyColumn}</th>
            <th scope="col">{t.crop.keyHints.actionColumn}</th>
          </tr>
        </thead>
        <tbody>
          {t.crop.keyHints.rows.map((row) => (
            <tr key={row.keys}>
              <td className="mono">{row.keys}</td>
              <td>{row.action}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="crop-key-hints-actions">
        <button
          ref={closeRef}
          type="button"
          className="btn btn-ghost"
          onClick={onClose}
        >
          {t.crop.keyHints.close}
        </button>
      </div>
    </ModalDialog>
  );
}
