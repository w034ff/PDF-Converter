import type { RefObject } from "react";
import { ConfirmDialog } from "../../components";
import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

export interface OrganizeClearDialogProps {
  onCancel: () => void;
  onConfirm: () => void;
  returnFocusRef: RefObject<HTMLElement | null>;
}

/**
 * Asks whether to remove every PDF after the pages were edited, which loses
 * their order and cannot be undone (organize §4, design §10.1).
 */
export function OrganizeClearDialog({
  onCancel,
  onConfirm,
  returnFocusRef,
}: OrganizeClearDialogProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language).organize.clearDialog;

  return (
    <ConfirmDialog
      title={t.title}
      cancelLabel={t.cancel}
      confirmLabel={t.confirm}
      onCancel={onCancel}
      onConfirm={onConfirm}
      returnFocusRef={returnFocusRef}
    >
      {t.body}
    </ConfirmDialog>
  );
}
