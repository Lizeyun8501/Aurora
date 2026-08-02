import {
  useEffect,
  useRef,
  type ReactElement,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import clsx from 'clsx';

export interface ModalProps {
  open: boolean;
  onClose?: () => void;
  title?: string;
  children: ReactNode;
  className?: string;
}

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])';

/** Accessible modal dialog: portal-based, basic focus trap, ESC + backdrop close. */
export function Modal({
  open,
  onClose,
  title,
  children,
  className,
}: ModalProps): ReactElement | null {
  const dialogRef = useRef<HTMLDivElement>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    previouslyFocused.current = (document.activeElement as HTMLElement | null) ?? null;
    const dialog = dialogRef.current;
    dialog?.focus();

    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose?.();
        return;
      }
      if (event.key === 'Tab' && dialog) {
        const focusable = Array.from(
          dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        const active = document.activeElement as HTMLElement | null;
        if (event.shiftKey && active === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && active === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      previouslyFocused.current?.focus?.();
    };
  }, [open, onClose]);

  if (!open) return null;

  const handleBackdrop = (event: React.MouseEvent<HTMLDivElement>): void => {
    if (event.target === event.currentTarget) onClose?.();
  };

  return createPortal(
    <div
      className="aurora-modal-backdrop"
      onMouseDown={handleBackdrop}
      role="presentation"
    >
      <div
        ref={dialogRef}
        className={clsx('aurora-modal', className)}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        {title && (
          <div className="aurora-modal-header">
            <h2 className="aurora-modal-title">{title}</h2>
            <button
              type="button"
              className="aurora-modal-close"
              aria-label="Close dialog"
              onClick={onClose}
            >
              ×
            </button>
          </div>
        )}
        <div className="aurora-modal-body">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
