import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from 'react';
import clsx from 'clsx';

export interface SplitPaneProps {
  left: ReactNode;
  right: ReactNode;
  /** Initial left-pane size as a percentage (0–100). Defaults to 50. */
  initialSize?: number;
  /** When set, the size is persisted to localStorage under this key. */
  storageKey?: string;
  className?: string;
}

const MIN_SIZE = 10;
const MAX_SIZE = 90;

function clampSize(n: number): number {
  return Math.min(MAX_SIZE, Math.max(MIN_SIZE, n));
}

/** Resizable two-pane (left/right) layout with a draggable divider. */
export function SplitPane({
  left,
  right,
  initialSize = 50,
  storageKey,
  className,
}: SplitPaneProps): ReactElement {
  const containerRef = useRef<HTMLDivElement>(null);

  const [size, setSize] = useState<number>(() => {
    if (storageKey && typeof localStorage !== 'undefined') {
      const stored = localStorage.getItem(storageKey);
      if (stored !== null) {
        const parsed = Number(stored);
        if (Number.isFinite(parsed)) return clampSize(parsed);
      }
    }
    return clampSize(initialSize);
  });

  useEffect(() => {
    if (storageKey && typeof localStorage !== 'undefined') {
      localStorage.setItem(storageKey, String(size));
    }
  }, [size, storageKey]);

  const handleMouseMove = useCallback((event: MouseEvent) => {
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const pct = ((event.clientX - rect.left) / rect.width) * 100;
    setSize(clampSize(pct));
  }, []);

  const handleMouseUp = useCallback(() => {
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', handleMouseUp);
  }, [handleMouseMove]);

  const handleMouseDown = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
    },
    [handleMouseMove, handleMouseUp],
  );

  // Clean up listeners on unmount.
  useEffect(() => {
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [handleMouseMove, handleMouseUp]);

  return (
    <div ref={containerRef} className={clsx('aurora-split-pane', className)}>
      <div className="aurora-split-pane-left" style={{ width: `${size}%` }}>
        {left}
      </div>
      <div
        className="aurora-split-pane-divider"
        role="separator"
        aria-orientation="vertical"
        onMouseDown={handleMouseDown}
      />
      <div className="aurora-split-pane-right" style={{ width: `${100 - size}%` }}>
        {right}
      </div>
    </div>
  );
}
