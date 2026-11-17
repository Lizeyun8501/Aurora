// C-05（TA 2026-11-17）: 空状态统一组件——全库唯一实现（MobileApp/CalendarView/
// PluginsView/DesktopShell 四处本地版退役）。组件零业务文案——title/hint/action
// 全场景传入（工单铁律）;插画位=内置线性 SVG 六场景 + custom 覆盖槽。
// 三态家族: EmptyState（本文件）/ Skeleton / ErrorState——样式同源自注入。
import type { ReactElement } from 'react';

export type EmptyIllustration = 'inbox' | 'search' | 'trash' | 'notes' | 'calendar' | 'generic';

export interface EmptyStateProps {
  illustration?: EmptyIllustration;
  /** 主文案（场景传入——组件不硬编码业务文案）。 */
  title: string;
  hint?: string;
  /** 单个主行动按钮（空态只留一个主行动）。 */
  actionLabel?: string;
  onAction?: () => void;
}

const STYLES = `
.aurora-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px 20px; gap: 4px; text-align: center; }
.aurora-empty-art { width: 56px; height: 56px; color: var(--text-secondary, #9aa4b2); opacity: .55; margin-bottom: 8px; }
.aurora-empty-title { font-size: 15px; font-weight: 600; color: var(--text-primary, #e7eaf0); margin: 0; }
.aurora-empty-hint { font-size: 13px; line-height: 1.6; color: var(--text-secondary, #9aa4b2); margin: 0; white-space: pre-line; }
.aurora-empty-btn { margin-top: 12px; min-height: 36px; padding: 8px 16px; border-radius: 8px; border: none; background: var(--brand, #2dd4bf); color: #06251f; font-size: 14px; font-weight: 600; cursor: pointer; }
.aurora-empty-btn:hover { background: var(--brand-pressed, #14b8a6); }
.aurora-skel { display: flex; flex-direction: column; gap: 8px; padding: 12px 0; }
.aurora-skel-row { height: 14px; border-radius: 6px; background: linear-gradient(90deg, rgba(148,163,184,.10) 25%, rgba(148,163,184,.20) 50%, rgba(148,163,184,.10) 75%); background-size: 200% 100%; animation: aurora-skel-wave 1.4s ease infinite; }
@keyframes aurora-skel-wave { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }
.aurora-err { display: flex; flex-direction: column; align-items: center; padding: 32px 20px; gap: 4px; text-align: center; }
.aurora-err-title { font-size: 15px; font-weight: 600; color: #f87171; margin: 0; }
.aurora-err-hint { font-size: 13px; color: var(--text-secondary, #9aa4b2); margin: 0 0 8px; }
.aurora-err-btn { min-height: 36px; padding: 8px 16px; border-radius: 8px; border: 1px solid rgba(255,255,255,.16); background: transparent; color: inherit; font-size: 14px; cursor: pointer; }
.aurora-err-btn:hover { background: rgba(255,255,255,.07); }
`;

let injected = false;
function ensureStyles(): void {
  if (injected || typeof document === 'undefined') return;
  if (!document.getElementById('aurora-state-styles')) {
    const el = document.createElement('style');
    el.id = 'aurora-state-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }
  injected = true;
}

/** 线性插画（一稿六景——stroke 1.5,64 viewBox;不做插画重绘=停手边界）。 */
function Art({ kind }: { kind: EmptyIllustration }): ReactElement {
  const common = { width: 56, height: 56, viewBox: '0 0 64 64', fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const, 'aria-hidden': true };
  switch (kind) {
    case 'inbox':
      return (<svg {...common}><path d="M8 32v18a4 4 0 0 0 4 4h40a4 4 0 0 0 4-4V32" /><path d="M8 32l8-18a4 4 0 0 1 3.7-2.5h24.6A4 4 0 0 1 48 14l8 18" /><path d="M8 32h14l4 6h16l4-6h14" /></svg>);
    case 'search':
      return (<svg {...common}><circle cx="27" cy="27" r="15" /><path d="M38 38l14 14" /><path d="M20 27a7 7 0 0 1 7-7" /></svg>);
    case 'trash':
      return (<svg {...common}><path d="M10 18h44" /><path d="M24 18v-6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v6" /><path d="M14 18l3 34a4 4 0 0 0 4 3.6h22a4 4 0 0 0 4-3.6l3-34" /><path d="M26 28v18M38 28v18" /></svg>);
    case 'calendar':
      return (<svg {...common}><rect x="10" y="14" width="44" height="42" rx="4" /><path d="M10 26h44M22 8v10M42 8v10" /><rect x="18" y="32" width="8" height="8" rx="1" /></svg>);
    case 'notes':
      return (<svg {...common}><rect x="14" y="8" width="36" height="48" rx="4" /><path d="M22 22h20M22 32h20M22 42h12" /></svg>);
    default:
      return (<svg {...common}><circle cx="32" cy="32" r="22" /><path d="M32 20v14l10 6" /></svg>);
  }
}

/** 空状态——全库唯一实现（DoD ②: 一份）。 */
export function EmptyState({ illustration = 'generic', title, hint, actionLabel, onAction }: EmptyStateProps): ReactElement {
  ensureStyles();
  return (
    <div className="aurora-empty" role="status">
      <div className="aurora-empty-art"><Art kind={illustration} /></div>
      <h3 className="aurora-empty-title">{title}</h3>
      {hint && <p className="aurora-empty-hint">{hint}</p>}
      {actionLabel && onAction && (
        <button type="button" className="aurora-empty-btn" onClick={onAction}>{actionLabel}</button>
      )}
    </div>
  );
}

/** 骨架屏——加载态统一（禁 spinner 转圈;行数/行宽场景化）。 */
export function Skeleton({ rows = 5, rowHeight = 14 }: { rows?: number; rowHeight?: number }): ReactElement {
  ensureStyles();
  return (
    <div className="aurora-skel" role="status" aria-label="加载中">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="aurora-skel-row" style={{ height: rowHeight, width: `${88 - (i % 3) * 18}%`, animationDelay: `${(i % 3) * 0.18}s` }} />
      ))}
    </div>
  );
}

/** 错误态——带重试;离线态不路由至此（V24.1: 离线不显示常驻错误条）。 */
export function ErrorState({ title = '加载失败', hint, onRetry }: { title?: string; hint?: string; onRetry?: () => void }): ReactElement {
  ensureStyles();
  return (
    <div className="aurora-err" role="alert">
      <h3 className="aurora-err-title">{title}</h3>
      {hint && <p className="aurora-err-hint">{hint}</p>}
      {onRetry && <button type="button" className="aurora-err-btn" onClick={onRetry}>重试</button>}
    </div>
  );
}
