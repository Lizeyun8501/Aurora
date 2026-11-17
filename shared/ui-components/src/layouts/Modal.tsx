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

/** C-05: 样式自注入——双端无 CSS 管线（desktop 无样式入口/mobile 有 css 但
 *  共享层不自增依赖）;模块级一次注入（id 去重）,双端一致观感。 */
const MODAL_STYLES = `
.aurora-modal-backdrop { position: fixed; inset: 0; z-index: 200; background: rgba(9, 12, 18, 0.62); display: flex; align-items: center; justify-content: center; padding: 16px; }
.aurora-modal { background: var(--bg-primary, #151a23); color: var(--text-primary, #e7eaf0); border: 1px solid rgba(255,255,255,0.1); border-radius: 12px; min-width: 320px; max-width: min(560px, 92vw); max-height: 84vh; display: flex; flex-direction: column; box-shadow: 0 12px 40px rgba(0,0,0,0.4); }
.aurora-modal-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 12px 16px; border-bottom: 1px solid rgba(255,255,255,0.08); }
.aurora-modal-title { margin: 0; font-size: 15px; font-weight: 600; }
.aurora-modal-close { border: none; background: transparent; color: inherit; font-size: 18px; line-height: 1; cursor: pointer; padding: 4px 8px; border-radius: 6px; min-width: 32px; min-height: 32px; }
.aurora-modal-close:hover { background: rgba(255,255,255,0.08); }
.aurora-modal-body { padding: 16px; overflow-y: auto; }
.aurora-dlg-msg { margin: 0 0 12px; font-size: 14px; line-height: 1.6; white-space: pre-wrap; word-break: break-word; }
.aurora-dlg-label { display: block; font-size: 12px; color: var(--text-secondary, #9aa4b2); margin-bottom: 6px; }
.aurora-dlg-input { width: 100%; box-sizing: border-box; padding: 8px 10px; font-size: 14px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.16); background: rgba(255,255,255,0.05); color: inherit; min-height: 36px; }
.aurora-dlg-input:focus { outline: 2px solid var(--brand, #2dd4bf); outline-offset: -1px; }
textarea.aurora-dlg-input { min-height: 72px; resize: vertical; font-family: inherit; }
.aurora-dlg-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.aurora-dlg-btn { min-height: 36px; padding: 8px 16px; font-size: 14px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.16); background: transparent; color: inherit; cursor: pointer; }
.aurora-dlg-btn:hover { background: rgba(255,255,255,0.07); }
.aurora-dlg-btn-primary { background: var(--brand, #2dd4bf); border-color: transparent; color: #06251f; font-weight: 600; }
.aurora-dlg-btn-primary:hover { background: var(--brand-pressed, #14b8a6); }
.aurora-dlg-btn-danger { background: #dc2626; border-color: transparent; color: #fff; font-weight: 600; }
.aurora-dlg-btn-danger:hover { background: #b91c1c; }
`;

let stylesInjected = false;
function ensureModalStyles(): void {
  if (stylesInjected || typeof document === 'undefined') return;
  if (!document.getElementById('aurora-modal-styles')) {
    const el = document.createElement('style');
    el.id = 'aurora-modal-styles';
    el.textContent = MODAL_STYLES;
    document.head.appendChild(el);
  }
  stylesInjected = true;
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
    /* C-05: 初始焦点=首个可聚焦子元素（input/按钮）——无则 dialog 容器。
     * （原 dialog.focus() 会覆盖子体聚焦——prompt 输入框失焦实测教训;
     * 对话框语义=落焦到交互面,容器兜底。） */
    if (dialog) {
      /* 输入控件优先（prompt 语义）→通用首可聚焦（header 关闭钮在 DOM
       * 前序会抢先——实测教训）→容器兜底。 */
      const first =
        dialog.querySelector<HTMLElement>('input, textarea, select') ??
        dialog.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
      (first ?? dialog).focus();
    }

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
  ensureModalStyles();

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
