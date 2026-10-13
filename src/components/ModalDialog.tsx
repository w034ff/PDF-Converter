import { useEffect, useRef, type ReactNode, type RefObject } from "react";

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), details > summary';

export interface ModalDialogProps {
  isOpen: boolean;
  onClose: () => void;
  titleId?: string;
  dialogClassName?: string;
  initialFocusRef?: RefObject<HTMLElement | null>;
  returnFocusRef?: RefObject<HTMLElement | null>;
  children: ReactNode;
}

/**
 * Reusable modal dialog frame component (design §10.1, §10.2).
 * Handles the backdrop overlay, focus trapping, Escape-to-close, and focus restoration.
 */
export function ModalDialog({
  isOpen,
  onClose,
  titleId,
  dialogClassName = "modal-dialog",
  initialFocusRef,
  returnFocusRef,
  children,
}: ModalDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  const triggerElementRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const active = document.activeElement;
    triggerElementRef.current =
      returnFocusRef?.current ??
      (active instanceof HTMLElement ? active : null);

    if (initialFocusRef?.current) {
      initialFocusRef.current.focus();
    } else {
      const dialog = dialogRef.current;
      if (dialog) {
        const focusable =
          dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
        if (focusable.length > 0) {
          focusable[0].focus();
        }
      }
    }
  }, [isOpen, initialFocusRef, returnFocusRef]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      const dialog = dialogRef.current;
      if (dialog === null) {
        return;
      }
      const focusable =
        dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      const outside = !dialog.contains(active);
      if (event.shiftKey && (active === first || outside)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || outside)) {
        event.preventDefault();
        first.focus();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [isOpen]);

  const wasOpenRef = useRef(isOpen);
  useEffect(() => {
    if (wasOpenRef.current && !isOpen) {
      const target = returnFocusRef?.current ?? triggerElementRef.current;
      if (
        target &&
        typeof target.focus === "function" &&
        document.contains(target)
      ) {
        target.focus();
      }
    }
    wasOpenRef.current = isOpen;

    const returnTarget = returnFocusRef?.current ?? triggerElementRef.current;
    return () => {
      if (wasOpenRef.current) {
        const target = returnTarget ?? triggerElementRef.current;
        if (
          target &&
          typeof target.focus === "function" &&
          document.contains(target)
        ) {
          target.focus();
        }
      }
    };
  }, [isOpen, returnFocusRef]);

  if (!isOpen) {
    return null;
  }

  return (
    <div
      className="modal-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onCloseRef.current();
        }
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className={dialogClassName}
      >
        {children}
      </div>
    </div>
  );
}
