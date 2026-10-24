import { Fragment, useRef, type RefObject } from "react";
import { CloseIcon, ModalDialog } from "../../components";
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
      <div className="modal-header">
        <h2 id="crop-key-hints-title" className="modal-title">
          {t.crop.keyHints.title}
        </h2>
        <button
          ref={closeRef}
          type="button"
          className="icon-btn modal-close"
          aria-label={t.crop.keyHints.close}
          onClick={onClose}
        >
          <CloseIcon size={16} />
        </button>
      </div>
      <div className="modal-body">
        <table className="crop-key-hints-table">
          {t.crop.keyHints.groups.map((group) => (
            <tbody key={group.title}>
              <tr>
                <th
                  scope="rowgroup"
                  colSpan={2}
                  className="crop-key-hints-group"
                >
                  {group.title}
                </th>
              </tr>
              {group.rows.map((row) => (
                <tr key={row.action}>
                  <td className="crop-key-hints-keys">
                    {row.keys.map((combo, comboIndex) => (
                      <Fragment key={combo.join("+")}>
                        {comboIndex > 0 && " / "}
                        <span className="crop-key-hints-combo">
                          {combo.map((key, keyIndex) => (
                            <Fragment key={key}>
                              {keyIndex > 0 && "+"}
                              <kbd>{key}</kbd>
                            </Fragment>
                          ))}
                        </span>
                      </Fragment>
                    ))}
                  </td>
                  <td>{row.action}</td>
                </tr>
              ))}
            </tbody>
          ))}
        </table>
      </div>
    </ModalDialog>
  );
}
