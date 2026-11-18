/**
 * C-04（TA 2026-11-18）: 笔记图标——单一事实源。
 *
 * 图标字段 = localStorage `aurora.icon.<id>`（E21 既有——不另造字段）;
 * 本模块自 mobile/primitives 上收共享层（树/书签/搜索/桌面封面四处消费
 * 同一实现——「图标三处一致」DoD 的结构前提）。
 */
import type { ReactElement } from 'react';

export interface IconDef { id: string; label: string; color: string; glyph: string }
export const BOOK_ICONS: IconDef[] = [
  { id: 'book-red', label: '红本', color: '#ef4444', glyph: '📖' },
  { id: 'book-blue', label: '蓝本', color: '#3b82f6', glyph: '📖' },
  { id: 'book-green', label: '绿本', color: '#10b981', glyph: '📖' },
  { id: 'book-amber', label: '橙本', color: '#f59e0b', glyph: '📖' },
  { id: 'book-purple', label: '紫本', color: '#8b5cf6', glyph: '📖' },
  { id: 'book-slate', label: '灰本', color: '#64748b', glyph: '📁' },
  { id: 'book-rose', label: '粉本', color: '#f43f5e', glyph: '📚' },
  { id: 'box-brown', label: '档案盒', color: '#a16207', glyph: '🗂' },
];
export const DOC_ICONS: IconDef[] = [
  { id: 'doc-plain', label: '文档', color: '#94a3b8', glyph: '📄' },
  { id: 'doc-note', label: '笔记', color: '#0ea5e9', glyph: '📝' },
  { id: 'doc-star', label: '星标', color: '#f59e0b', glyph: '⭐' },
  { id: 'doc-idea', label: '灵感', color: '#a855f7', glyph: '💡' },
  { id: 'doc-task', label: '待办', color: '#10b981', glyph: '☑' },
  { id: 'doc-diary', label: '日记', color: '#f43f5e', glyph: '📔' },
  { id: 'doc-code', label: '代码', color: '#334155', glyph: '⌨' },
  { id: 'doc-chart', label: '图表', color: '#6366f1', glyph: '📊' },
];
export const ALL_ICONS = [...BOOK_ICONS, ...DOC_ICONS];

export function getIconDef(kind: 'book' | 'doc', id: string): IconDef {
  try {
    const saved = localStorage.getItem(`aurora.icon.${id}`);
    if (saved) { const hit = ALL_ICONS.find((x) => x.id === saved); if (hit) return hit; }
  } catch { /* 防御 */ }
  return kind === 'book' ? BOOK_ICONS[0] : DOC_ICONS[0];
}

export function setIconDef(id: string, iconId: string): void {
  try { localStorage.setItem(`aurora.icon.${id}`, iconId); } catch { /* 防御 */ }
}

/** VIVO 图标块: 圆角方底色+白字形（20px 线性档=视觉克制口径——C-04）。 */
export function VivoIcon({ def, size = 20 }: { def: IconDef; size?: number }): ReactElement {
  return (
    <span className="vivo-icon" aria-label={def.label} title={def.label}
      style={{ width: size, height: size, background: def.color, fontSize: Math.round(size * 0.58), flex: '0 0 auto' }}>
      {def.glyph}
    </span>
  );
}
