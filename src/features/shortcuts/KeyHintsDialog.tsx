import { Fragment, useRef, type RefObject } from "react";
import { CloseIcon, ModalDialog } from "../../components";
import type { Translations } from "../../i18n";

/** The wording of a list of keys: the one the crop tab and the organize tab both have. */
export type KeyHintsDictionary = Translations["crop"]["keyHints"];

export interface KeyHintsDialogProps {
  hints: KeyHintsDictionary;
  /** Names the title for the dialog; distinct for each tab's list. */
  titleId: string;
  onClose: () => void;
  /** Where the focus goes back to once the list is closed. */
  returnFocusRef: RefObject<HTMLElement | null>;
}

/**
 * The keys of a tab, listed in a dialog (design §10.1): none of them show on
 * the screen otherwise.
 */
export function KeyHintsDialog({
  hints,
  titleId,
  onClose,
  returnFocusRef,
}: KeyHintsDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);

  return (
    <ModalDialog
      isOpen={true}
      onClose={onClose}
      titleId={titleId}
      dialogClassName="modal-dialog crop-key-hints"
      initialFocusRef={closeRef}
      returnFocusRef={returnFocusRef}
    >
      <div className="modal-header">
        <h2 id={titleId} className="modal-title">
          {hints.title}
        </h2>
        <button
          ref={closeRef}
          type="button"
          className="icon-btn modal-close"
          aria-label={hints.close}
          onClick={onClose}
        >
          <CloseIcon size={16} />
        </button>
      </div>
      <div className="modal-body">
        <table className="crop-key-hints-table">
          {hints.groups.map((group) => (
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
