import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { platform, type NoteSummary, type SearchResult, type CalEvent } from './adapters/androidPlatform';
import SyncStatusBadge from './components/SyncStatusBadge';
import QueuePanel from './components/QueuePanel';
import ConflictReviewPanel from './components/ConflictReviewPanel';
import { Solar, HolidayUtil } from 'lunar-typescript';
import CalendarView from './views/CalendarView';
import { expandAll } from './views/repeatUtil';

// 编辑器懒加载 — schema/wasm 初始化失败不拖垮整个应用（白屏防御）
// DK-0F editor-uplift：三件套上移共享层，mobile 侧零本地编辑器实现（真实 import）
const RichEditor = React.lazy(() =>
  import('@aurora/ui-components').then((m) => ({ default: m.RichEditor })),
);
// 平台快照桥注入（共享层不感知 android adapter — DK-0F editor-uplift）
const editorPlatform = platform as unknown as import('@aurora/ui-components').EditorPlatformBridge;

// ===========================================================================
// V19 §1.2 图标 — 20px 线性简约，无渐变无填充
// ===========================================================================

export const I = {
  today: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/></svg>
  ),
  notes: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M4 4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z"/><path d="M9 2v20M13 7h4M13 11h4"/></svg>
  ),
  search: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/></svg>
  ),
  agent: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="4" y="8" width="16" height="12" rx="2"/><path d="M12 8V4M8 4h8M9 13h.01M15 13h.01"/></svg>
  ),
  gear: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
  ),
  plus: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M12 5v14M5 12h14"/></svg>
  ),
  back: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m15 18-6-6 6-6"/></svg>
  ),
  chev: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m9 18 6-6-6-6"/></svg>
  ),
  chevDown: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg>
  ),
  more: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M12 5h.01M12 12h.01M12 19h.01"/></svg>
  ),
  check: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
  ),
  clock: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>
  ),
  trash: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/></svg>
  ),
  sync: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M21 12a9 9 0 0 1-9 9 9 9 0 0 1-7.9-4.7M3 12a9 9 0 0 1 9-9 9 9 0 0 1 7.9 4.7"/><path d="M21 3v5h-5M3 21v-5h5"/></svg>
  ),
  wifi: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><circle cx="12" cy="20" r="1" fill="currentColor" stroke="none"/></svg>
  ),
  note: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z"/><path d="M14 2v5h6"/></svg>
  ),
  x: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
  ),
  lock: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>
  ),
  moon: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/></svg>
  ),
  sun: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>
  ),
  spark: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"/></svg>
  ),
  text: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M4 6h16M4 12h12M4 18h8"/></svg>
  ),
  camera: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3z"/><circle cx="12" cy="13" r="3"/></svg>
  ),
  mic: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 10v1a7 7 0 0 0 14 0v-1M12 19v3"/></svg>
  ),
  focus: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3M8 21H5a2 2 0 0 1-2-2v-3M16 21h3a2 2 0 0 0 2-2v-3"/></svg>
  ),
  menu: (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round"><path d="M4 6h16M4 12h16M4 18h16"/></svg>
  ),
  star: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m12 2 3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z"/></svg>
  ),
  book: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/></svg>
  ),
  calendar: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/></svg>
  ),
  taskSquare: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="m9 12 2 2 4-4"/></svg>
  ),
  user: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="8" r="4"/><path d="M5 21a7 7 0 0 1 14 0"/></svg>
  ),
  restore: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/></svg>
  ),
  diaryPen: (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>
  ),
};

// ===========================================================================
// 全局状态
// ===========================================================================

type ViewId = 'today' | 'notes' | 'search' | 'agent' | 'settings' | 'calendar'
  | 'favorites' | 'recent' | 'tasks' | 'diary' | 'trash';
const VIEW_TITLES: Record<ViewId, string> = {
  today: '今日', notes: '知识库', search: '搜索', agent: 'Agent', settings: '设置', calendar: '日历',
  favorites: '我的收藏', recent: '最近浏览', tasks: '任务中心', diary: '日记本', trash: '回收站',
};
/** 次级视图 → Tab 高亮映射（抽屉导航进入次级页时保持主 Tab 语境）。
 *  2026-10-28 搜索入口合并：底部 TabBar 撤搜索项，入口唯一=AppBar 右上角——
 *  search 视图无主 Tab 高亮（搜索是全局动作，不属于任何主 Tab 语境）。 */
const TAB_OF_VIEW: Record<ViewId, string> = {
  today: 'today', notes: 'notes', agent: 'agent', settings: 'settings',
  calendar: 'calendar', favorites: 'notes', recent: 'notes', diary: 'notes', trash: 'notes', tasks: 'tasks', search: '',
};

// ===== E21（TA 21:57）: VIVO 风格图标集 — 彩色圆角块+白字形;overlay 持久可换 =====
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
const ALL_ICONS = [...BOOK_ICONS, ...DOC_ICONS];
function getIconDef(kind: 'book' | 'doc', id: string): IconDef {
  try {
    const saved = localStorage.getItem(`aurora.icon.${id}`);
    if (saved) { const hit = ALL_ICONS.find((x) => x.id === saved); if (hit) return hit; }
  } catch { /* 防御 */ }
  return kind === 'book' ? BOOK_ICONS[0] : DOC_ICONS[0];
}
function setIconDef(id: string, iconId: string) {
  try { localStorage.setItem(`aurora.icon.${id}`, iconId); } catch { /* 防御 */ }
}
/** VIVO 图标块: 圆角方底色+白字形（16-22px 视觉等效 emoji 但可控配色） */
function VivoIcon({ def, size = 22 }: { def: IconDef; size?: number }) {
  return (
    <span className="vivo-icon" aria-label={def.label} title={def.label}
      style={{ width: size, height: size, background: def.color, fontSize: Math.round(size * 0.58) }}>
      {def.glyph}
    </span>
  );
}

/** 笔记标题 overlay — 编辑页改标题时覆盖显示（Rust rename API 落地前的前端方案）。 */
function getTitleOverlay(id: string): string | null {
  try { return localStorage.getItem(`aurora.title.${id}`); } catch { return null; }
}
function setTitleOverlay(id: string, title: string) {
  try { localStorage.setItem(`aurora.title.${id}`, title); } catch { /* 防御 */ }
}
/** 统一列表入口 — 原始 listNotes + 标题 overlay 合并。 */
export function listNotesTitled(): NoteSummary[] {
  return platform.listNotes().map((n) => ({ ...n, title: getTitleOverlay(n.id) || n.title }));
}

/** 收藏 / 最近浏览 / 回收站 — localStorage（Rust 侧元数据表落地后迁移 platform API）。 */
interface TrashItem { id: string; title: string; text: string; deletedAt: number; }
interface RecentItem { id: string; title: string; at: number; }

function loadList<T>(k: string): T[] {
  try { return JSON.parse(localStorage.getItem(k) ?? '[]'); } catch { return []; }
}
function saveList<T>(k: string, v: T[]) {
  try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* 配额满/隐私模式 */ }
}
const K_FAVS = 'aurora.favs';
const K_RECENT = 'aurora.recent';
const K_TRASH = 'aurora.trash';
/** E27 目录级别 overlay（TA 23:16）: 0=顶级;>0 大纲层级（缩进显示）——
 * localStorage 覆盖层（同 icon/title/order 惯例,Rust 元数据表落地后迁移）。 */
const levelKey = (id: string) => `aurora.level.${id}`;
function getLevelOverlay(id: string): number {
  try { return Math.max(0, Math.min(3, Number(localStorage.getItem(levelKey(id)) ?? 0) || 0)); } catch { return 0; }
}
function setLevelOverlay(id: string, lv: number) {
  try { localStorage.setItem(levelKey(id), String(Math.max(0, Math.min(3, lv)))); } catch { /* 防御 */ }
}

/** 手动排序 overlay（VIVO 原子笔记 v4·长按拖拽排序对标）——
 * core list_notes 返回 updated_at 序（3a8545 冻结面不可动）;
 * localStorage 覆盖层存 id 序列,非命中项按 core 序垫底。
 * 同 title overlay 惯例（Rust 元数据表落地后迁移）。 */
const K_ORDER = 'aurora.noteOrder';
function loadOrder(): string[] {
  try { return JSON.parse(localStorage.getItem(K_ORDER) ?? '[]') as string[]; } catch { return []; }
}
function saveOrder(ids: string[]) {
  try { localStorage.setItem(K_ORDER, JSON.stringify(ids)); } catch { /* 防御 */ }
}
/** 应用手动序: order 命中段按 overlay 序前置,余项按原序垫底。 */
export function applyManualOrder<T extends { id: string }>(notes: T[], order: string[]): T[] {
  if (!order.length) return notes;
  const pos = new Map(order.map((id, i) => [id, i]));
  return [...notes].sort((a, b) => {
    const pa = pos.get(a.id) ?? Number.MAX_SAFE_INTEGER;
    const pb = pos.get(b.id) ?? Number.MAX_SAFE_INTEGER;
    if (pa !== pb) return pa - pb;
    return 0; // 同未命中保 stable sort 原序
  });
}

/** 任务 — GTD2.0 四分区（V19 §二页面1）。localStorage 演示级存储，
 *  Rust 侧任务表落地后切换 platform API。 */
export type TaskStatus = 'next' | 'waiting' | 'plan' | 'done';
export type TaskPriority = 'high' | 'normal' | 'low';
export const TASK_PRIO: Array<{ id: TaskPriority; label: string; dot: string }> = [
  { id: 'high', label: '高', dot: 'var(--danger)' },
  { id: 'normal', label: '中', dot: 'var(--brand)' },
  { id: 'low', label: '低', dot: 'var(--text-tertiary)' },
];
export interface Task {
  id: string; text: string; status: TaskStatus;
  due?: string; noteId?: string; createdAt: number;
  /** E17 任务中心增强（TA 19:30）: 优先级三档——排序权重+色点 */
  priority?: TaskPriority;
  /** V26 M2/DK-06 时间追踪: 预估/实际分钟（actual 只能经番茄钟累加） */
  estimateMinutes?: number;
  actualMinutes?: number;
  /** 子任务归属（有父的行显示进度条 — progress 由子任务推导, 禁止手改） */
  parentTaskId?: string;
  progress?: number;
}
export const TASK_GROUPS: Array<{ id: TaskStatus; label: string }> = [
  { id: 'next', label: '下一步行动' },
  { id: 'waiting', label: '等待' },
  { id: 'plan', label: '计划' },
  { id: 'done', label: '已完成' },
];

/** V26 M2/DK-06: 周回顾汇总 — FFI 优先, localStorage 演示级降级。 */
function focusSummary(tasks: Task[]): { estimate: number; actual: number; deviation: number | null } {
  let estimate = 0, actual = 0;
  for (const t of tasks) {
    actual += t.actualMinutes ?? 0;
    estimate += t.estimateMinutes ?? 0;
  }
  const remote = (platform as { todayFocusSummary?: () => { estimate: number; actual: number; deviation: number | null } | null }).todayFocusSummary?.();
  if (remote) return remote;
  const deviation = estimate > 0 ? (actual - estimate) / estimate : null;
  return { estimate, actual, deviation };
}

/** V26 M2/DK-06: 预计 vs 实际徽标 — 偏差率着色（|d|>0.3 警示）。
 *  FFI 周回顾汇总优先; localStorage 演示级降级; 双零隐藏。 */
function FocusBadge({ tasks }: { tasks: Task[] }) {
  // V26 M2/DK-06: Rust 投影真源优先（Android 桥），FFI 不可用降级本地推导
  const remote = useMemo(() => platform.todayFocusSummary(), []);
  const { estimate, actual, deviation } = remote ?? focusSummary(tasks);
  if (estimate === 0 && actual === 0) return null;
  const devText = deviation === null ? '' :
    deviation === 0 ? '（持平）' : '（' + (deviation > 0 ? '+' : '') + Math.round(deviation * 100) + '%）';
  const warn = deviation !== null && Math.abs(deviation) > 0.3;
  return (
    <span
      className="focus-badge"
      title="预计 vs 实际（周回顾）"
      style={{ color: warn ? 'var(--warning, #b45309)' : 'var(--text-tertiary)' }}
    >
      预计 {estimate}′ · 实际 {actual}′{devText}
    </span>
  );
}

function loadTasks(): Task[] {
  try { return JSON.parse(localStorage.getItem('aurora.tasks') ?? '[]'); } catch { return []; }
}
function saveTasks(ts: Task[]) {
  try { localStorage.setItem('aurora.tasks', JSON.stringify(ts)); } catch { /* 配额满/隐私模式 */ }
}

export interface ToastAction { text: string; actionLabel: string; onAction: () => void }
function useToast() {
  const [msg, setMsg] = useState<string | ToastAction | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const show = useCallback((t: string | ToastAction) => {
    setMsg(t);
    clearTimeout(timerRef.current);
    // E18: action 版长驻 5s（留点击窗）;纯文本 2.2s 原节奏
    timerRef.current = setTimeout(() => setMsg(null), typeof t === 'string' ? 2200 : 5000);
  }, []);
  useEffect(() => () => clearTimeout(timerRef.current), []);
  const dismiss = useCallback(() => { clearTimeout(timerRef.current); setMsg(null); }, []);
  return { msg, show, dismiss };
}

// ===========================================================================
// 应用骨架 — 底部 Tab（V19 §3.1）+ 悬浮捕获（永不隐藏）
// ===========================================================================


// ===========================================================================
// DK-05M 键盘协同 — visualViewport 唯一方案（Alpha 裁决①②）
// ===========================================================================

/**
 * 键盘视口感知：visualViewport resize/scroll → rAF 节流写 CSS 变量。
 * - `--vvh`：可视视口高度（app-shell 高度用它，禁 100vh 键盘态用法）
 * - `--kbd-inset`：键盘遮挡高度（工具条 fixed 贴底用；收起态回退 safe-area 由 CSS max() 处理）
 * 半接入审计：MobileApp 此前零 resize/visualViewport 监听，无执行顺序短路风险（D1 教训排查）。
 */
function useKbdViewport(): void {
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return; // 桌面浏览器无 vv：CSS 回退值天然生效
    let raf = 0;
    const apply = () => {
      raf = 0;
      const root = document.documentElement.style;
      root.setProperty('--vvh', `${Math.round(vv.height)}px`);
      const inset = Math.max(0, window.innerHeight - vv.height - vv.offsetTop);
      root.setProperty('--kbd-inset', `${Math.round(inset)}px`);
    };
    const schedule = () => {
      if (!raf) raf = requestAnimationFrame(apply);
    };
    vv.addEventListener('resize', schedule, { passive: true });
    vv.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule, { passive: true });
    apply();
    return () => {
      vv.removeEventListener('resize', schedule);
      vv.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      if (raf) cancelAnimationFrame(raf);
    };
  }, []);
}

export default function App() {
  useKbdViewport(); // DK-05M：键盘视口感知（--vvh/--kbd-inset）
  const [ready, setReady] = useState(false);
  const [fallback, setFallback] = useState(false);
  const [view, setView] = useState<ViewId>('today'); // V19: TodayView 默认启动页
  /** 2026-10-28 周条→日历联动：待跳转选中日期（YYYY-MM-DD），CalendarView 挂载时消费 */
  const [calendarInit, setCalendarInit] = useState<string | null>(null);
  /** 11-08 23:10 用户令: 待办单击跳转——日历直落 day 页签（挂载时消费）。 */
  const [calendarModeInit, setCalendarModeInit] = useState<'day' | null>(null);
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(null);
  const [captureOpen, setCaptureOpen] = useState(false);
  const [captureDue, setCaptureDue] = useState<string | undefined>(undefined); // 日历入口预置日期
  const [drawerOpen, setDrawerOpen] = useState(false);
  // DK-50：同步两面板浮层开关（badge 三入口 + menu-pop 两入口共用）
  const [queueOpen, setQueueOpen] = useState(false);
  const [conflictOpen, setConflictOpen] = useState(false);
  // DK-50.1：面板决策时间戳（bump 触发 badge 即刷——联动打通）
  const [syncChangedAt, setSyncChangedAt] = useState(0);
  // E10b（TA 14:03）: 主题三态——light/dark/system（跟随系统）
  type ThemeMode = 'light' | 'dark' | 'system';
  const [theme, setTheme] = useState<ThemeMode>(() =>
    (localStorage.getItem('aurora.theme') as ThemeMode) ?? 'light');
  const [sysDark, setSysDark] = useState(() =>
    window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false);
  // 系统切换实时随（matchMedia 监听——system 模式即时生效）
  useEffect(() => {
    const mq = window.matchMedia?.('(prefers-color-scheme: dark)');
    if (!mq) return;
    const onChange = (e: MediaQueryListEvent) => setSysDark(e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);
  const [tasks, setTasks] = useState<Task[]>(loadTasks);
  // DK-CALENDAR-02：日程事件（mock/真桥双路径——platform.listEvents）
  const [events, setEvents] = useState<CalEvent[]>([]);
  const [favs, setFavs] = useState<string[]>(() => loadList<string>(K_FAVS));
  const [recents, setRecents] = useState<RecentItem[]>(() => loadList<RecentItem>(K_RECENT));
  const [trash, setTrash] = useState<TrashItem[]>(() => loadList<TrashItem>(K_TRASH));
  const { msg: toastMsg, show: showToast, dismiss: dismissToast } = useToast();

  useEffect(() => {
    try { localStorage.setItem('aurora.theme', theme); } catch { /* 防御 */ }
    // E10b: system→按 matchMedia 解析（CSS data-theme 面恒二态,兼容既有暗色覆写零改）
    document.documentElement.dataset.theme = theme === 'system' ? (sysDark ? 'dark' : 'light') : theme;
  }, [theme, sysDark]);

  useEffect(() => {
    const ok = platform.init('/data/aurora');
    setReady(true);
    setFallback(!ok || platform.isFallback());
    // DK-CALENDAR-02：事件初始化加载（mock/真桥双路径）
    try { setEvents(platform.listEvents()); } catch { setEvents([]); }
  }, []);

  /** DK-CALENDAR-02：事件 upsert/删除后刷新列表。 */
  const refreshEvents = () => {
    try { setEvents(platform.listEvents()); } catch { /* 降级保旧态 */ }
  };

  if (!ready) return <SplashScreen />;

  const updateTasks = (ts: Task[]) => { setTasks(ts); saveTasks(ts); };

  /** 打开笔记 — 记录最近浏览。 */
  const openNote = (id: string, title: string) => {
    setRecents((prev) => {
      const next = [{ id, title, at: Date.now() }, ...prev.filter((r) => r.id !== id)].slice(0, 20);
      saveList(K_RECENT, next);
      return next;
    });
    setEditing({ id, title });
  };

  /** 收藏切换。 */
  const toggleFav = (id: string) => {
    setFavs((prev) => {
      const next = prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id];
      saveList(K_FAVS, next);
      showToast(prev.includes(id) ? '已取消收藏' : '已收藏');
      return next;
    });
  };

  /** 软删除 — 内容存入回收站后调 Rust 删除（回收站可恢复文本）。 */
  const softDelete = (id: string, title: string) => {
    let text = '';
    try { text = platform.getNoteContent(id); } catch { /* ignore */ }
    setTrash((prev) => {
      const next = [{ id, title, text, deletedAt: Date.now() }, ...prev.filter((t) => t.id !== id)];
      saveList(K_TRASH, next);
      return next;
    });
    // mock 桥对称（S2-B）: aurora.trash Dto 侧同步落（真机 bridge 时 deleteNote core 自落——此写幂等无害）
    try {
      const dtos = JSON.parse(localStorage.getItem('aurora.trash') ?? '[]') as Array<{ noteId: string; title?: string; deletedAtMs?: number; originPath?: string | null }>;
      if (!dtos.some((d) => d.noteId === id)) {
        dtos.unshift({ noteId: id, title, deletedAtMs: Date.now(), originPath: null });
        localStorage.setItem('aurora.trash', JSON.stringify(dtos));
      }
    } catch { /* 防御 */ }
    setFavs((prev) => { const n = prev.filter((x) => x !== id); saveList(K_FAVS, n); return n; });
    setRecents((prev) => { const n = prev.filter((r) => r.id !== id); saveList(K_RECENT, n); return n; });
    // E27a: 删除失败可见化——假成功是「无法删除」观感最大嫌疑（E15 share reject 吞病同源）
    if (!platform.deleteNote(id)) {
      showToast('删除失败（核心未删）——请重试');
      return; // 失败=不进假成功链（trash/favs/recents 不动）
    }
    snippetCache.delete(id);
  };

  const goto = (v: ViewId) => { setView(v); setDrawerOpen(false); };

  return (
    <div className="app-shell">
      {editing ? (
        <NoteEditor
          noteId={editing.id}
          title={editing.title}
          isFav={favs.includes(editing.id)}
          onToggleFav={() => toggleFav(editing.id)}
          onClose={() => {
            snippetCache.delete(editing.id);
            setEditing(null);
          }}
          onDeleted={() => { softDelete(editing.id, editing.title); showToast('已移入回收站'); setEditing(null); }}
          onOpenQueue={() => setQueueOpen(true)}
          onOpenConflicts={() => setConflictOpen(true)}
          showToast={showToast}
        />
      ) : (
        <>
          <AppBar
            title={VIEW_TITLES[view]}
            fallback={fallback}
            onMenu={() => setDrawerOpen(true)}
            onSearch={() => goto('search')}
            onAgent={() => goto('agent')}
            syncBadge={
              <SyncStatusBadge
                onOpenQueue={() => setQueueOpen(true)}
                onOpenConflicts={() => setConflictOpen(true)}
                lastChangedAt={syncChangedAt}
              />
            }
          />
          <main className="content">
            {view === 'today' && (
              <TodayView
                tasks={tasks}
                events={events}
                onTasks={updateTasks}
                onOpenNote={openNote}
                showToast={showToast}
                onOpenDate={(ds) => { setCalendarInit(ds); goto('calendar'); }}
              />
            )}
            {view === 'notes' && (
              <NotesView
                onOpen={openNote}
                showToast={showToast}
                onNewNote={() => setCaptureOpen(true)}
                onSoftDelete={softDelete}
                favs={favs}
                onToggleFav={toggleFav}
                setNoteEncryption={(id, level) => platform.setNoteEncryption(id, level)}
              />
            )}
            {view === 'search' && <SearchView onOpen={openNote} />}
            {view === 'calendar' && (
              <CalendarView
                tasks={tasks}
                events={events}
                onUpsertEvent={(ev) => { platform.upsertEvent(ev); refreshEvents(); platform.refreshReminders(); }}
                onDeleteEvent={(id) => { platform.deleteEvent(id); refreshEvents(); platform.refreshReminders(); }}
                onOpenNote={openNote}
                onNewTask={(due) => { setCaptureDue(due); setCaptureOpen(true); }}
                initialSelected={calendarInit ?? undefined}
                initialMode={calendarModeInit ?? undefined}
              />
            )}
            {view === 'agent' && <AgentView theme={theme} />}
            {view === 'settings' && (
              <SettingsView theme={theme} onTheme={setTheme} showToast={showToast} />
            )}
            {view === 'favorites' && <FavoritesView favs={favs} onOpen={openNote} />}
            {view === 'recent' && <RecentView recents={recents} onOpen={openNote} />}
            {view === 'tasks' && (
              <TasksView tasks={tasks} onTasks={updateTasks} showToast={showToast}
                /* 11-08 23:10 用户令: 单击已有待办→跳日历对应日 day 页签（due 缺省=今日）。 */
                onOpenTask={(t) => {
                  const n = new Date();
                  const ds = t.due ?? `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
                  setCalendarInit(ds); setCalendarModeInit('day'); goto('calendar');
                }} />
            )}
            {view === 'diary' && <DiaryView onOpen={openNote} showToast={showToast} />}
            {view === 'trash' && (
              <TrashView trash={trash} onTrash={setTrash} showToast={showToast} />
            )}
          </main>

          {/* 悬浮捕获 — 永不隐藏（**日历页除外**——CALENDAR-VIVO 11-01: 日历 FAB 属日历域,
              由 CalendarView 渲染（日程+任务双入口——不吃笔记捕获流）;其余页保持笔记捕获。 */}
          {view !== 'calendar' && (
            <button className="fab" onClick={() => setCaptureOpen(true)} aria-label="捕获">
              {I.plus}
            </button>
          )}

          <TabBar current={view} onChange={(v) => setView(v as ViewId)} />

          {drawerOpen && (
            <Drawer2
              current={view}
              onNavigate={goto}
              onClose={() => setDrawerOpen(false)}
              tasks={tasks}
              favs={favs}
              trash={trash}
            />
          )}
        </>
      )}

      {/* DK-50：同步两面板（全屏 sheet；onChanged→syncChangedAt bump→badge 即刷） */}
      {queueOpen && (
        <QueuePanel
          onClose={() => setQueueOpen(false)}
          onChanged={() => setSyncChangedAt(Date.now())}
        />
      )}
      {conflictOpen && (
        <ConflictReviewPanel
          onClose={() => setConflictOpen(false)}
          onChanged={() => setSyncChangedAt(Date.now())}
        />
      )}

      {captureOpen && (
        <CaptureSheet
          onClose={() => { setCaptureOpen(false); setCaptureDue(undefined); }}
          onNewNote={(title, body) => {
            const id = platform.createNote(title);
            if (!id) return;
            if (body) platform.saveNoteContent(id, body);
            setCaptureOpen(false);
            setEditing({ id, title });
          }}
          onNewTask={(text, status, priority) => {
            // E17: 「加入今日任务」语义——captureDue 缺省时 due=今日（否则永远落「稍后安排」组）
            const n = new Date();
            const today = `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
            updateTasks([...tasks, {
              id: `t-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
              text, status, createdAt: Date.now(), due: captureDue ?? today, priority,
            }]);
            setCaptureOpen(false);
            setCaptureDue(undefined);
            setView('today');
            showToast(captureDue ? `任务已计划到 ${captureDue}` : '已添加到今日任务');
          }}
          onNewEvent={(title) => {
            // DK-CALENDAR-02：今日 09:00 预填日程（captureDue 优先——日历入口带日期）
            const n = new Date();
            const today = `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
            const day = captureDue ?? today;
            platform.upsertEvent({
              id: '', title, start: `${day}T09:00:00`, end: null,
              all_day: false, location: null, note_id: null, color: 1,
            });
            refreshEvents();
            setCaptureOpen(false);
            setCaptureDue(undefined);
            setView('calendar');
            showToast(`日程已添加到 ${day} 09:00`);
          }}
        />
      )}

      {toastMsg && (
        <div className={`toast ${typeof toastMsg !== 'string' ? 'toast-action' : ''}`}>
          {typeof toastMsg === 'string' ? toastMsg : (
            <>
              <span className="toast-action-text">{toastMsg.text}</span>
              <button className="toast-action-btn" onClick={() => { toastMsg.onAction(); dismissToast(); }}>{toastMsg.actionLabel}</button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

// ===========================================================================
// AppBar — 汉堡 + 标题 + 搜索
// ===========================================================================

function AppBar({ title, fallback, onMenu, onSearch, onAgent, syncBadge }: {
  title: string; fallback: boolean; onMenu: () => void; onSearch: () => void;
  /** E16（TA 19:13）: Agent 入口上移 app-bar（搜索旁）——底部 nav 位让给「待办」 */
  onAgent: () => void;
  /** DK-50：同步状态徽标（app-bar 常驻，不破现有布局） */
  syncBadge?: React.ReactNode;
}) {
  return (
    <header className="app-bar">
      <button className="icon-btn" onClick={onMenu} aria-label="菜单">{I.menu}</button>
      <h1 className="app-bar-title">{title}</h1>
      {fallback && <span className="app-bar-warn">内存模式</span>}
      {syncBadge}
      <button className="icon-btn" onClick={onAgent} aria-label="Agent">{I.agent}</button>
      <button className="icon-btn" onClick={onSearch} aria-label="搜索">{I.search}</button>
    </header>
  );
}

function SplashScreen() {
  return (
    <div className="splash">
      <div className="splash-logo">Aurora</div>
      <div className="splash-tip">本地优先 · P2P 同步</div>
    </div>
  );
}

function TabBar({ current, onChange }: { current: ViewId; onChange: (v: ViewId) => void }) {
  const items: Array<{ id: ViewId; icon: React.ReactNode; label: string }> = [
    { id: 'today', icon: I.today, label: '今日' },
    { id: 'notes', icon: I.notes, label: '知识库' },
    { id: 'tasks', icon: I.taskSquare, label: '待办' },
    { id: 'calendar', icon: I.calendar, label: '日历' },
  ];
  return (
    <nav className="tab-bar">
      {items.map((t) => (
        <button
          key={t.id}
          className={`tab-item ${TAB_OF_VIEW[current] === t.id ? 'active' : ''}`}
          onClick={() => onChange(t.id)}
        >
          {t.icon}
          <span>{t.label}</span>
        </button>
      ))}
    </nav>
  );
}

// ===========================================================================
// TodayView — V19 移动端页面1（默认启动页 · P1）
// ===========================================================================

function TodayView({ tasks, events, onTasks, onOpenNote, showToast, onOpenDate }: {
  tasks: Task[];
  /** DK-CALENDAR-04 任务 6: 今日事件块进「日程」分区（Event 按 start 落——与 Task 并列）。 */
  events: CalEvent[];
  onTasks: (ts: Task[]) => void;
  onOpenNote: (id: string, title: string) => void;
  showToast: (t: string) => void;
  /** 2026-10-28 周条可点选日——跳日历页选中该日（用户实测「本周内无法点击」反馈） */
  onOpenDate: (dateStr: string) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({ done: true });
  const [pomoRunning, setPomoRunning] = useState(false);
  const [pomoSec, setPomoSec] = useState(25 * 60);

  // 番茄钟 — 计时纯减法，完成判定移出 updater（副作用归 effect）
  useEffect(() => {
    if (!pomoRunning) return;
    const t = setInterval(() => setPomoSec((s) => Math.max(0, s - 1)), 1000);
    return () => clearInterval(t);
  }, [pomoRunning]);

  useEffect(() => {
    if (pomoRunning && pomoSec === 0) {
      setPomoRunning(false);
      setPomoSec(25 * 60);
      showToast('番茄钟完成 🍅');
    }
  }, [pomoRunning, pomoSec, showToast]);

  const today = new Date();
  const dateStr = `${today.getMonth() + 1} 月 ${today.getDate()} 日`;
  const weekDay = ['日', '一', '二', '三', '四', '五', '六'][today.getDay()];
  const doneToday = tasks.filter((t) => t.status === 'done').length;
  const openCount = tasks.filter((t) => t.status !== 'done').length;

  const notes = listNotesTitled();
  const weekNotes = notes.slice(0, 3);

  const toggle = (id: string) => {
    onTasks(tasks.map((t) =>
      t.id === id ? { ...t, status: t.status === 'done' ? 'next' : 'done' } : t));
  };

  // 左滑删除
  const swipe = useRef<{ x: number; id: string | null }>({ x: 0, id: null });

  return (
    <div className="view">
      <header className="today-header">
        <div className="today-date">{dateStr} · 周{weekDay}</div>
        <div className="today-sub">
          <span className="sync-badge"><span className="sync-dot ok" />已同步</span>
          <span>{openCount} 项待办 · {doneToday} 项已完成</span>
          <FocusBadge tasks={tasks} />
        </div>
      </header>

      {/* AI 今日洞察 — 本地统计摘要（无 AI 后端时诚实降级） */}
      <div className="card insight-card">
        <span className="insight-icon">{I.spark}</span>
        <div className="insight-text">
          今日已完成 <b>{doneToday}</b> 项任务，待办 <b>{openCount}</b> 项。
          本周更新 {notes.length} 篇笔记。
          <span style={{ color: 'var(--text-tertiary)' }}>（本地统计 · Agent 未启用）</span>
        </div>
      </div>

      {/* DK-CALENDAR-04 任务 6: 日程分区——今日事件块（展开实例;与 Task 并列——PRD 今日备忘 P0）。 */}
      {(() => {
        const todayKey = (() => {
          const n = new Date();
          return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
        })();
        const insts = expandAll(events, new Date(`${todayKey}T00:00:00`), new Date(`${todayKey}T23:59:59`));
        insts.sort((a, b) => a.start.getTime() - b.start.getTime());
        if (insts.length === 0) return null;
        return (
          <div className="task-section open">
            <button className="task-section-head" aria-label="日程分区">
              <span>今日日程</span><span className="count">{insts.length}</span>
            </button>
            <div className="ev-list-group">
              {insts.map((inst) => (
                <button key={`${inst.ev.id}@${inst.offsetDays}`} className="ev-list-item" role="button"
                  onClick={() => onOpenDate(todayKey)}>
                  <i className="ev-list-dot" style={{ background: ['#dc2626', '#2563eb', '#059669', '#d97706', '#7c3aed', '#6b7280'][inst.ev.color % 6] }} />
                  <span className="ev-list-time">{inst.ev.all_day ? '全天' : `${String(inst.start.getHours()).padStart(2, '0')}:${String(inst.start.getMinutes()).padStart(2, '0')}`}</span>
                  <span className="ev-list-title">{inst.ev.title}</span>
                </button>
              ))}
            </div>
          </div>
        );
      })()}

      {/* 任务分区 — GTD2.0 四组可折叠 */}
      {TASK_GROUPS.map((g) => {
        const items = tasks.filter((t) => t.status === g.id);
        if (g.id !== 'done' && items.length === 0) {
          return (
            <div key={g.id} className="task-section collapsed">
              <button className="task-section-head" onClick={() => setCollapsed({ ...collapsed, [g.id]: !collapsed[g.id] })}>
                <span>{g.label}</span><span className="count">空</span><span className="chev">{I.chev}</span>
              </button>
            </div>
          );
        }
        return (
          <div key={g.id} className={`task-section ${collapsed[g.id] ? 'collapsed' : 'open'}`}>
            <button className="task-section-head" onClick={() => setCollapsed({ ...collapsed, [g.id]: !collapsed[g.id] })}>
              <span>{g.label}</span>
              <span className="count">{items.length} 项</span>
              <span className="chev">{I.chev}</span>
            </button>
            <div className="task-section-body">
              {items.map((t) => (
                <div
                  key={t.id}
                  className={`task-item ${t.status === 'done' ? 'done' : ''}`}
                  onTouchStart={(e) => { swipe.current = { x: e.touches[0].clientX, id: t.id }; }}
                  onTouchMove={(e) => {
                    const dx = e.touches[0].clientX - swipe.current.x;
                    if (swipe.current.id === t.id && dx < -60) swipe.current.id = `del:${t.id}`;
                  }}
                  onTouchEnd={() => {
                    if (swipe.current.id === `del:${t.id}`) {
                      onTasks(tasks.filter((x) => x.id !== t.id));
                      showToast('任务已删除');
                    }
                    swipe.current = { x: 0, id: null };
                  }}
                >
                  <button
                    className={`task-check ${t.status === 'done' ? 'checked' : ''}`}
                    onClick={() => toggle(t.id)}
                    aria-label="完成任务"
                  >{I.check}</button>
                  <div className="task-body">
                    <div className="task-text">{t.text}</div>
                    <div className="task-meta">
                      {t.due && <span className="task-due">{t.due}</span>}
                      {t.noteId && (
                        <button className="task-note-link" onClick={() => {
                          const n = listNotesTitled().find((x) => x.id === t.noteId);
                          if (n) onOpenNote(n.id, n.title);
                        }}>{I.note} 关联笔记</button>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        );
      })}

      {/* 本周日历 — 2026-10-28 可点选日（原静态展示 div，用户实测点击无反应） */}
      <div className="card">
        <h3 className="card-title">本周</h3>
        <div className="week-calendar">
          {Array.from({ length: 7 }, (_, i) => {
            const d = new Date(today);
            d.setDate(d.getDate() - d.getDay() + i + 1); // 周一起
            const isToday = d.toDateString() === today.toDateString();
            const ds = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
            return (
              <button
                key={i}
                type="button"
                className={`week-day ${isToday ? 'today' : ''}`}
                onClick={() => onOpenDate(ds)}
                aria-label={`查看 ${ds}`}
              >
                <span>{['一', '二', '三', '四', '五', '六', '日'][i]}</span>
                <span className="d">{d.getDate()}</span>
                <span className="week-dot has" />
              </button>
            );
          })}
        </div>
      </div>

      {/* 重点笔记 */}
      {weekNotes.length > 0 && (
        <div className="card">
          <h3 className="card-title">最近笔记</h3>
          {weekNotes.map((n) => (
            <div key={n.id} className="mini-note" onClick={() => onOpenNote(n.id, n.title)}>
              <span className="mini-note-title">{n.title}</span>
              <span className="mini-note-time">{relativeTime(n.updatedAt)}</span>
            </div>
          ))}
        </div>
      )}

      {/* 番茄钟 */}
      <div className="card">
        <h3 className="card-title">专注</h3>
        <div className="pomodoro">
          <div className={`pomo-time ${pomoRunning ? 'running' : ''}`}>
            {String(Math.floor(pomoSec / 60)).padStart(2, '0')}:{String(pomoSec % 60).padStart(2, '0')}
          </div>
          <div className="pomo-btns">
            <button className="btn btn-primary" onClick={() => setPomoRunning(!pomoRunning)}>
              {pomoRunning ? '暂停' : '开始专注'}
            </button>
            <button className="btn btn-secondary" onClick={() => { setPomoRunning(false); setPomoSec(25 * 60); }}>
              重置
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ===========================================================================
// 捕获 & AI 液化 — V19 移动端页面2（悬浮 + 唤起底部抽屉 70%）
// ===========================================================================

function CaptureSheet({ onClose, onNewNote, onNewTask, onNewEvent }: {
  onClose: () => void;
  onNewNote: (title: string, body: string) => void;
  onNewTask: (text: string, status: TaskStatus, priority: TaskPriority) => void;
  /** DK-CALENDAR-02 任务 3：FAB 捕获流「日程」类型（今日 09:00 预填）。 */
  onNewEvent: (title: string) => void;
}) {
  const [mode, setMode] = useState<'text' | 'ocr' | 'voice'>('text');
  const [text, setText] = useState('');
  const [prio, setPrio] = useState<TaskPriority>('normal'); // E17: 捕获即定优先级

  // AI 预处理（本地启发式演示 — 推荐 PARA 分类 + 提取行动任务）
  const aiHint = (() => {
    const t = text.trim();
    if (!t) return null;
    const isAction = /(完成|处理|回复|提交|联系|整理|修复|部署|review|fix)/i.test(t);
    const category = isAction ? 'Projects' : 'Resources';
    return { category, isAction };
  })();

  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">快速捕获</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          <div className="capture-modes">
            <button className={`capture-mode ${mode === 'text' ? 'active' : ''}`} onClick={() => setMode('text')}>
              {I.text}<span>文字录入</span>
            </button>
            <button className={`capture-mode ${mode === 'ocr' ? 'active' : ''}`} onClick={() => setMode('ocr')}>
              {I.camera}<span>图片 OCR</span>
            </button>
            <button className={`capture-mode ${mode === 'voice' ? 'active' : ''}`} onClick={() => setMode('voice')}>
              {I.mic}<span>语音速记</span>
            </button>
          </div>

          {mode === 'text' && (
            <>
              {/* E17 任务中心增强: 捕获即定优先级（三段）——high 排前/色点可视 */}
              <div className="task-prio-row" role="radiogroup" aria-label="优先级">
                {TASK_PRIO.map((p) => (
                  <button key={p.id} className={`task-prio-seg ${prio === p.id ? 'on' : ''}`}
                    role="radio" aria-checked={prio === p.id} onClick={() => setPrio(p.id)}>
                    <span className="task-prio-dot" style={{ background: p.dot }} />{p.label}
                  </button>
                ))}
              </div>
              <textarea
                className="input"
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="想到什么写什么，AI 自动整理…"
                autoFocus
              />
              {aiHint && (
                <div className="capture-ai-tip">
                  🤖 AI 预处理建议 — 推荐分类 <b>{aiHint.category}</b>
                  {aiHint.isAction ? ' · 识别为行动任务，可加入今日' : ' · 适合归档为笔记'}
                </div>
              )}
              <div className="capture-actions">
                <button
                  className="btn btn-primary"
                  disabled={!text.trim()}
                  style={{ opacity: text.trim() ? 1 : 0.5 }}
                  onClick={() => {
                    const t = text.trim();
                    onNewNote(t.slice(0, 24), t);
                  }}
                >生成笔记</button>
                <button
                  className="btn btn-secondary"
                  disabled={!text.trim()}
                  style={{ opacity: text.trim() ? 1 : 0.5 }}
                  onClick={() => onNewTask(text.trim().slice(0, 60), 'next', prio)}
                >加入今日任务</button>
                <button
                  className="btn btn-secondary"
                  disabled={!text.trim()}
                  style={{ opacity: text.trim() ? 1 : 0.5 }}
                  onClick={() => onNewEvent(text.trim().slice(0, 60))}
                >存为日程</button>
              </div>
            </>
          )}
          {mode === 'ocr' && (
            <div className="empty-state">
              <div className="empty-text">图片 OCR 捕获即将上线<br />当前版本可先用文字录入</div>
            </div>
          )}
          {mode === 'voice' && (
            <div className="empty-state">
              <div className="empty-text">语音速记即将上线<br />当前版本可先用文字录入</div>
            </div>
          )}
        </div>
      </div>
    </>
  );
}

// ===========================================================================
// 知识库 — V19 移动端页面3（列表 + 摘要 + 任务红点 + 左滑删除）
// ===========================================================================

/** 时间机器快照列表 — JNI listSnapshots/getSnapshotContent 桥（V23-I5 移动端）。 */
function SnapshotList({ noteId, refreshKey }: { noteId: string; refreshKey: number }) {
  const [metas, setMetas] = useState<Array<{ version: number; created_at: string; size: number }>>([]);
  const [viewing, setViewing] = useState<{ version: number; content: string } | null>(null);

  useEffect(() => {
    try {
      const anyPlatform = platform as unknown as Record<string, unknown>;
      if (typeof anyPlatform.listSnapshots !== 'function') return; // 非 Android 环境守卫
      const arr = JSON.parse(platform.listSnapshots(noteId) || '[]');
      setMetas(Array.isArray(arr) ? arr : []);
    } catch { setMetas([]); }
  }, [noteId, refreshKey]);

  if (!metas.length) {
    return <div style={{ fontSize: 12, opacity: 0.6, marginTop: 6 }}>暂无快照（保存后自动生成）</div>;
  }
  return (
    <div style={{ marginTop: 8 }}>
      {metas.map((m) => (
        <div
          key={m.version}
          className="settings-row"
          style={{ padding: '8px 0', borderTop: '1px solid var(--border)' }}
          onClick={() => {
            try {
              const c = platform.getSnapshotContent(noteId, m.version);
              setViewing({ version: m.version, content: c ?? '（读取失败）' });
            } catch { setViewing({ version: m.version, content: '（读取失败）' }); }
          }}
        >
          <span className="settings-row-label">
            版本 {m.version}
            <div className="sub">{m.created_at} · {m.size} 字节 · 点按查看存档正文</div>
          </span>
          <span className="settings-value">查看</span>
        </div>
      ))}
      {viewing && (
        <>
          <div className="sheet-mask" onClick={() => setViewing(null)} />
          <div className="sheet">
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">版本 {viewing.version} 存档</span>
              <button className="icon-btn" onClick={() => setViewing(null)} aria-label="关闭">{I.x}</button>
            </div>
            <div className="sheet-body">
              <pre style={{ whiteSpace: 'pre-wrap', fontSize: 14, lineHeight: 1.6, fontFamily: 'inherit', margin: 0 }}>
                {viewing.content}
              </pre>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

/** 笔记摘要缓存（body 前 60 字）。 */
const snippetCache = new Map<string, string>();
function noteSnippet(id: string): string {
  const cached = snippetCache.get(id);
  if (cached !== undefined) return cached;
  let s = '';
  try { s = platform.getNoteContent(id).replace(/\s+/g, ' ').trim().slice(0, 60); } catch { /* 兜底 */ }
  snippetCache.set(id, s);
  return s;
}

// ===========================================================================
// 笔记本文档树 — V23-I5（思源式无限级目录）
// 树元数据 localStorage（doc 节点 id = noteId, 内容复用现有笔记体系）;
// core parent_id schema 下沉 + P2P 同步树结构 → V24 路线（日记记录）
// ===========================================================================

interface TreeNodeMeta {
  id: string;              // doc = noteId; book = 'b<ts>'（纯组织节点）
  kind: 'book' | 'doc';
  title: string;
  parentId: string | null; // book 为 null
}

const K_TREE = 'aurora.tree';
function loadTree(): TreeNodeMeta[] {
  try { return JSON.parse(localStorage.getItem(K_TREE) || 'null') || []; } catch { return []; }
}
function saveTree(t: TreeNodeMeta[]) {
  try { localStorage.setItem(K_TREE, JSON.stringify(t)); } catch { /* 防御 */ }
}

/** 首次进入: 自动建「我的笔记本」; 现有平铺笔记走孤儿区「入树」, 不自动迁移（时序安全）。 */
function initialTree(): TreeNodeMeta[] {
  return [{ id: `b${Date.now()}`, kind: 'book', title: '我的笔记本', parentId: null }];
}
/** E21 修: 初始树立即落盘（K_TREE）——否则 tab 切换 NodeTree 重挂→loadTree() 空→
 *  initialTree 生成新 b<ts> id,图标 overlay（aurora.icon.<id>）与旧 id 失联=换图标不持久。 */
function initialTreePersisted(): TreeNodeMeta[] {
  const t = initialTree();
  saveTree(t);
  return t;
}

/** 文档树组件 — 展开折叠 + ＋新建子文档 + 孤儿笔记收入 + E21 图标更换。 */
function NodeTree({ notes, onOpen, showToast, onSoftDelete }: {
  notes: NoteSummary[];
  onOpen: (id: string, title: string) => void;
  showToast: (m: string) => void;
  onSoftDelete: (id: string, title: string) => void;
}) {
  // E21: 图标选择器（sheet: 全集 16 格;选中落 overlay 持久,树行即时刷新）
  const [pick, setPick] = useState<{ id: string; kind: 'book' | 'doc' } | null>(null);
  const [, setIconTick] = useState(0);
  const onPickIcon = (id: string, kind: 'book' | 'doc') => setPick({ id, kind });
  const applyIcon = (iconId: string) => {
    if (!pick) return;
    setIconDef(pick.id, iconId);
    setIconTick((t) => t + 1); // 树行重读 overlay
    setPick(null);
    showToast('图标已更换');
  };
  const [tree, setTree] = useState<TreeNodeMeta[]>(() => {
    const t = loadTree();
    return t.length ? t : initialTreePersisted();
  });
  const persist = (t: TreeNodeMeta[]) => { saveTree(t); setTree(t); };
  // ===== E27（TA 23:16）: 树节点长按——上下文菜单/拖动移父/升降级 =====
  const [ctxId, setCtxId] = useState<string | null>(null);
  const [treeDrag, setTreeDrag] = useState<string | null>(null); // 拖动节点 id
  const [treeDragOver, setTreeDragOver] = useState<string | null>(null); // 落点行 id
  const press = useRef<ReturnType<typeof setTimeout> | null>(null);
  const ctxNode = tree.find((t) => t.id === ctxId) ?? null;

  const siblings = (pid: string | null) => tree.filter((t) => t.parentId === pid);
  /** 移节点到 target 行的父下、target 行后一位（拖动=移动+换父——层级变换一次覆盖）。 */
  const moveNode = (dragId: string, targetId: string) => {
    const drag = tree.find((t) => t.id === dragId);
    const target = tree.find((t) => t.id === targetId);
    if (!drag || !target || dragId === targetId) return;
    // 祖先环防: target 是 drag 的后代→拒（自吞树）
    let anc: string | null = target.parentId;
    while (anc) { if (anc === dragId) { showToast('不能移到自己的子节点下'); return; } anc = tree.find((t) => t.id === anc)?.parentId ?? null; }
    const newParent = target.kind === 'book' ? target.id : target.parentId;
    if (drag.parentId === newParent && treeDragOver === dragId) return; // 同位免动
    const rest = tree.filter((t) => t.id !== dragId);
    const ti = rest.findIndex((t) => t.id === targetId);
    const moved = { ...drag, parentId: newParent };
    rest.splice(ti + 1, 0, moved);
    persist(rest);
    showToast('已移动');
  };
  /** 升一级: 挂到原父的父下（原父同级后）。顶级/book 无法升。 */
  const levelUp = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n || !n.parentId) { showToast('已是顶级'); return; }
    const parent = tree.find((t) => t.id === n.parentId);
    if (!parent) { showToast('已是顶级'); return; }
    if (parent.kind === 'book' && !parent.parentId) {
      // 父是顶级笔记本→升为顶级 book? doc 不能成 book——语义边界: 移为顶级孤儿 doc 不合理,拒
      showToast('已是最上层（父为顶级笔记本）');
      return;
    }
    const rest = tree.filter((t) => t.id !== id);
    const pi = rest.findIndex((t) => t.id === parent.id);
    rest.splice(pi + 1, 0, { ...n, parentId: parent.parentId });
    persist(rest);
    showToast('已升一级');
  };
  /** 降一级: 挂到前一个同级节点下（成其末子）。无前同级→拒。 */
  const levelDown = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n) return;
    const sibs = siblings(n.parentId).filter((t) => t.id !== id);
    const idx = tree.findIndex((t) => t.id === id);
    const prevSame = [...tree.slice(0, idx)].reverse().find((t) => t.parentId === n.parentId && t.id !== id);
    if (!prevSame) { showToast('前面没有同级节点可挂靠'); return; }
    if (prevSame.kind !== 'book' && prevSame.id === id) return;
    const rest = tree.filter((t) => t.id !== id);
    rest.push({ ...n, parentId: prevSame.id });
    persist(rest);
    showToast(`已挂到「${prevSame.kind === 'book' ? prevSame.title : getTitleOverlay(prevSame.id) || prevSame.title}」下`);
  };
  /** 删除: doc→软删（回收站）+摘树;book→摘树（子升孤儿）。 */
  const removeNode = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n) return;
    persist(tree.filter((t) => t.id !== id));
    if (n.kind === 'doc') {
      onSoftDelete(n.id, displayTitle(n));
      showToast('已移入回收站');
    } else {
      showToast('笔记本已移除（子笔记转入未入树区）');
    }
  };

  const [open, setOpen] = useState<Set<string>>(() => new Set(loadTree().filter((x) => x.kind === 'book').map((x) => x.id)));

  const toggle = (id: string) =>
    setOpen((prev) => { const n = new Set(prev); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const childrenOf = (pid: string) => tree.filter((t) => t.parentId === pid);
  const firstBookId = tree.find((t) => t.kind === 'book')?.id ?? null;
  const displayTitle = (t: TreeNodeMeta) => (t.kind === 'doc' ? getTitleOverlay(t.id) || t.title : t.title);

  /** 新建子文档: 走现有 createNote（内容体系不动）, 树挂节点, 就地创建不跳转（对齐思源）。 */
  const createChild = (parentId: string) => {
    const id = platform.createNote('未命名文档');
    if (!id) { showToast('创建失败'); return; }
    persist([...tree, { id, kind: 'doc', title: '未命名文档', parentId }]);
    setOpen((prev) => new Set(prev).add(parentId));
    showToast('已创建子文档（点标题行改名）');
  };

  /** 平铺孤儿笔记收入第一个笔记本。 */
  const adopt = (n: NoteSummary) => {
    if (!firstBookId) return;
    persist([...tree, { id: n.note_id, kind: 'doc', title: n.title, parentId: firstBookId }]);
    showToast(`「${n.title}」已收入笔记本`);
  };

  const treeDocIds = new Set(tree.filter((t) => t.kind === 'doc').map((t) => t.id));
  const orphans = notes.filter((n) => !treeDocIds.has(n.note_id));

  const row = (t: TreeNodeMeta, depth: number): React.ReactNode => {
    const kids = childrenOf(t.id);
    const hasKids = kids.length > 0;
    const isOpen = open.has(t.id);
    return (
      <React.Fragment key={t.id}>
        <div
          className={`nt-row ${t.kind === 'book' ? 'nt-book' : ''} ${treeDrag === t.id ? 'dragging' : ''} ${treeDragOver === t.id && treeDrag && treeDrag !== t.id ? 'drag-over' : ''}`}
          style={{ paddingLeft: 8 + depth * 18 }}
          data-node-id={t.id}
          onTouchStart={(e) => {
            if (treeDrag) return; // 拖动态不加新长按
            if (press.current) clearTimeout(press.current);
            press.current = setTimeout(() => { setCtxId(t.id); }, 400);
          }}
          onTouchMove={() => {
            if (press.current) { clearTimeout(press.current); press.current = null; }
            if (treeDrag && treeDrag !== t.id) setTreeDragOver(t.id);
          }}
          onTouchEnd={(e) => {
            if (press.current) { clearTimeout(press.current); press.current = null; }
            // E27: 长按刚弹菜单——阻合成 click 穿 backdrop 秒关（touchend preventDefault 阻 click 合成）
            if (ctxId) { e.preventDefault(); return; }
            if (treeDrag) {
              if (treeDragOver && treeDragOver !== treeDrag) moveNode(treeDrag, treeDragOver);
              setTreeDrag(null); setTreeDragOver(null);
            }
          }}
        >
          {hasKids || t.kind === 'book' ? (
            <button className="nt-arrow" onClick={() => toggle(t.id)} aria-label={isOpen ? '折叠' : '展开'}>{isOpen ? '▾' : '▸'}</button>
          ) : <span className="nt-arrow nt-arrow-ph" />}
          {/* E21: VIVO 图标块——点击开选择器换图标（overlay 持久） */}
          <button className="nt-icon-btn" aria-label="更换图标" title="更换图标"
            onClick={() => onPickIcon?.(t.id, t.kind)}><VivoIcon def={getIconDef(t.kind, t.id)} /></button>
          <button className="nt-title" onClick={() => t.kind === 'doc' && onOpen(t.id, displayTitle(t))}>
            {displayTitle(t)}
            {t.kind === 'doc' && hasKids && <span className="nt-count">{kids.length}</span>}
          </button>
          <button className="nt-add" onClick={() => createChild(t.id)} aria-label="新建子文档">＋</button>
        </div>
        {isOpen && kids.map((k) => row(k, depth + 1))}
      </React.Fragment>
    );
  };

  return (
    <div>
      {treeDrag && <div className="tree-drag-tip">拖动中——移到目标行松手（落点行即新位置/新层级）</div>}
      {tree.filter((t) => t.kind === 'book').map((b) => row(b, 0))}
      <button className="btn btn-text" style={{ marginTop: 4 }} onClick={() => {
        const id = `b${Date.now()}`;
        const name = `笔记本 ${tree.filter((t) => t.kind === 'book').length + 1}`;
        persist([...tree, { id, kind: 'book', title: name, parentId: null }]);
        showToast(`已创建「${name}」`);
      }}>＋ 新建笔记本</button>

      {orphans.length > 0 && (
        <>
          <div className="nt-orphan-title">未入树笔记（{orphans.length}）— 点「收入」挂到笔记本</div>
          {orphans.map((n) => (
            <div key={n.note_id} className="nt-row" style={{ paddingLeft: 8 }}>
              <span className="nt-arrow nt-arrow-ph" />
              <button className="nt-icon-btn" aria-label="更换图标" title="更换图标"
                onClick={() => onPickIcon?.(n.note_id, 'doc')}><VivoIcon def={getIconDef('doc', n.note_id)} /></button>
              <button className="nt-title" onClick={() => onOpen(n.note_id, n.title)}>{n.title}</button>
              <button className="nt-add" style={{ fontSize: 12 }} onClick={() => adopt(n)}>收入</button>
            </div>
          ))}
        </>
      )}

      {/* E27（TA 23:16）: 树节点长按菜单——删除/换图标/升降级/拖动排序 */}
      {ctxNode && (
        <div className="ctx-sheet-backdrop" onClick={() => setCtxId(null)}>
          <div className="ctx-sheet" role="menu" aria-label="节点操作" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">
              {ctxNode.kind === 'book' ? '📁' : '📄'} {displayTitle(ctxNode)}
            </div>
            <button className="ctx-item danger" onClick={() => { setCtxId(null); removeNode(ctxNode.id); }}>🗑 删除{ctxNode.kind === 'book' ? '笔记本（子笔记转未入树区）' : '（移入回收站）'}</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); onPickIcon(ctxNode.id, ctxNode.kind); }}>🎨 更换图标</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); levelUp(ctxNode.id); }}>↖ 升一级</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); levelDown(ctxNode.id); }}>↘ 降一级</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); setTreeDrag(ctxNode.id); showToast('拖到目标行松手（换位置/换层级）'); }}>⇅ 拖动排序</button>
            <button className="ctx-item ctx-cancel" onClick={() => setCtxId(null)}>取消</button>
          </div>
        </div>
      )}

      {/* E21 图标选择器 sheet: 16 格全集（book/doc 8+8）;点击即换+toast */}
      {pick && (
        <>
          <div className="sheet-mask" onClick={() => setPick(null)} />
          <div className="sheet" style={{ paddingBottom: 14 }}>
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">更换图标</span>
              <button className="icon-btn" onClick={() => setPick(null)} aria-label="关闭">{I.x}</button>
            </div>
            <div style={{ padding: '6px 12px 0' }}>
              <div style={{ fontSize: 12, color: 'var(--text-tertiary)', marginBottom: 8 }}>{pick.kind === 'book' ? '笔记本' : '文档'}图标</div>
              <div className="vivo-icon-grid">
                {(pick.kind === 'book' ? BOOK_ICONS : DOC_ICONS).map((ic) => (
                  <button key={ic.id} className="vivo-icon-cell" title={ic.label} aria-label={ic.label}
                    onClick={() => applyIcon(ic.id)}>
                    <VivoIcon def={ic} size={34} />
                  </button>
                ))}
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function NotesView({ onOpen, showToast, onNewNote, onSoftDelete, favs, onToggleFav, setNoteEncryption }: {
  onOpen: (id: string, title: string) => void;
  showToast: (t: string) => void;
  onNewNote: () => void;
  onSoftDelete: (id: string, title: string) => void;
  favs: string[];
  onToggleFav: (id: string) => void;
  setNoteEncryption: (id: string, level: 'none' | 'aes256gcm') => boolean;
}) {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [query, setQuery] = useState('');
  const [swipingId, setSwipingId] = useState<string | null>(null);
  const [mode, setMode] = useState<'tree' | 'list'>('tree');
  /** v4 长按拖拽排序（VIVO 原子笔记对标）: 拖起 id / 落点索引 / 长按定时器 / 手动序 overlay。 */
  const [dragId, setDragId] = useState<string | null>(null);
  // E27（TA 23:16）: 长按上下文菜单——删除/换图标/升降级/拖动排序
  const [ctxId, setCtxId] = useState<string | null>(null);
  const [ctxIconPick, setCtxIconPick] = useState(false); // 换图标宫格展开态
  const [uiVer, setUiVer] = useState(0); // overlay（level/icon）变更 bump——重渲染
  const [dragOverIdx, setDragOverIdx] = useState<number | null>(null);
  const pressTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [order, setOrder] = useState<string[]>(() => loadOrder());

  const refresh = useCallback(() => setNotes(listNotesTitled()), []);
  useEffect(refresh, [refresh]);

  /** 左滑删除（V19 移动端页面1 触屏交互）
   * ⚠️ hook 必须在 mode 条件 early return 之前——树形轮曾跳过此声明，
   * 切列表时 hooks 数量 3→4 触发 React #310 渲染崩溃（v24 真机首报）。 */
  const cardTouch = useRef<{ x: number; y: number } | null>(null);

  /** 视图段控 — 树形（思源式笔记本/文档树）/ 列表（原平铺）。 */
  const modeSeg = (
    <div style={{ display: 'flex', gap: 6, marginBottom: 12 }}>
      <button className={`nt-seg ${mode === 'tree' ? 'on' : ''}`} onClick={() => setMode('tree')}>树形</button>
      <button className={`nt-seg ${mode === 'list' ? 'on' : ''}`} onClick={() => setMode('list')}>列表</button>
    </div>
  );

  if (mode === 'tree') {
    return (
      <div className="view">
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
          <span style={{ fontSize: 20, fontWeight: 700 }}>文档</span>
          {modeSeg}
        </div>
        <NodeTree notes={notes} onOpen={onOpen} showToast={showToast} onSoftDelete={onSoftDelete} />
      </div>
    );
  }

  const ordered = applyManualOrder(notes, order); // v4 手动序（搜索态保持过滤后原序——手动序仅主导无搜索态）
  const shown = query.trim()
    ? notes.filter((n) => n.title.includes(query.trim()))
    : ordered;
  // E27: ctxNote 在 shown 后求值（声明序）
  const ctxNote = ctxId ? shown.find((n) => n.id === ctxId) ?? notes.find((n) => n.id === ctxId) : null;

  const onMove = (e: React.TouchEvent, id: string) => {
    if (!cardTouch.current) return;
    const dx = e.touches[0].clientX - cardTouch.current.x;
    if (dx < -64) setSwipingId(id); // 左滑露删除/加密（既有语义）
    if (dx > 64) setSwipingId(`fav:${id}`); // 右滑露收藏（VIVO 原子笔记·右滑设重要度对标）
  };

  /** v4 长按拖拽: touchstart 起 350ms 定时器（未到期 touchmove=滑动取消）;拖起后 touchmove 换落点索引。 */
  const onCardTouchStart = (e: React.TouchEvent, id: string) => {
    cardTouch.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };
    if (pressTimer.current) clearTimeout(pressTimer.current);
    // E27: 长按=弹上下文菜单（拖动排序经菜单入口——VIVO 惯例;旧「长按直接拖」废止）
    pressTimer.current = setTimeout(() => { setCtxId(id); setCtxIconPick(false); }, 400);
  };
  const onCardTouchMove = (e: React.TouchEvent, id: string, idx: number) => {
    if (!cardTouch.current) return;
    const dx = e.touches[0].clientX - cardTouch.current.x;
    const dy = e.touches[0].clientY - cardTouch.current.y;
    if (dragId === null) {
      // 非拖动态: 位移>10 即取消长按判定（滑动 vs 长按二分）
      if (Math.abs(dx) > 10 || Math.abs(dy) > 10) {
        if (pressTimer.current) { clearTimeout(pressTimer.current); pressTimer.current = null; }
        if (dx < -64) setSwipingId(id);
        if (dx > 64) setSwipingId(`fav:${id}`);
      }
      return;
    }
    // 拖动态: 记落点
    if (dragId !== id) setDragOverIdx(idx);
  };
  /** E27 长按菜单操作。 */
  const ctxStartDrag = (id: string) => { setCtxId(null); setCtxIconPick(false); setDragId(id); showToast('拖动到目标位置后松手'); };
  const ctxLevel = (id: string, delta: number) => {
    const cur = getLevelOverlay(id);
    const next = Math.max(0, Math.min(3, cur + delta));
    if (next === cur) { showToast(delta > 0 ? '已是最深层级' : '已是顶级'); return; }
    setLevelOverlay(id, next);
    setUiVer((v) => v + 1);
    showToast(`层级: ${next === 0 ? '顶级' : `L${next}`}`);
  };
  const ctxDelete = (id: string, title: string) => {
    setCtxId(null); setCtxIconPick(false);
    onSoftDelete(id, title);
    showToast('已移入回收站');
  };
  const ctxSetIcon = (id: string, iconId: string) => {
    setIconDef(id, iconId);
    setUiVer((v) => v + 1);
    setCtxIconPick(false);
    setCtxId(null);
    showToast('图标已更换');
  };
  const onCardTouchEnd = () => {
    if (pressTimer.current) { clearTimeout(pressTimer.current); pressTimer.current = null; }
    cardTouch.current = null;
    if (dragId !== null) {
      // 落定: 以 dragOverIdx 为目标位重建序（未记录落点=原位取消）
      if (dragOverIdx !== null && dragOverIdx >= 0) {
        const ids = applyManualOrder(notes, order).map((n) => n.id);
        const arr = ids.filter((x) => x !== dragId);
        const to = Math.min(dragOverIdx, arr.length);
        arr.splice(to, 0, dragId);
        setOrder(arr);
        saveOrder(arr);
        showToast('顺序已保存');
      }
      setDragId(null);
      setDragOverIdx(null);
    }
  };

  return (
    <div className="view">
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
        <span style={{ fontSize: 20, fontWeight: 700 }}>知识库</span>
        {modeSeg}
      </div>
      <div className="search-hero-input" style={{ marginBottom: 12 }}>
        <span style={{ color: 'var(--text-tertiary)' }}>{I.search}</span>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="筛选笔记…"
        />
      </div>

      {shown.length === 0 ? (
        <div className="empty-state">
          <div className="empty-text">
            {query ? '没有匹配的笔记' : '新建笔记 / 从收件箱导入素材'}
            {!query && (
              <><br /><button className="btn btn-text" onClick={onNewNote}>+ 新建笔记</button></>
            )}
          </div>
        </div>
      ) : (
        <div className="note-list">
          {shown.map((n, idx) => (
            <div
              key={n.id}
              className={`note-card ${swipingId === n.id || swipingId === `fav:${n.id}` ? 'swiped' : ''} ${swipingId === `fav:${n.id}` ? 'swiped-fav' : ''} ${dragId === n.id ? 'dragging' : ''} ${dragOverIdx === idx && dragId !== null && dragId !== n.id ? 'drag-over' : ''}`}
              onTouchStart={(e) => onCardTouchStart(e, n.id)}
              onTouchMove={(e) => onCardTouchMove(e, n.id, idx)}
              onTouchEnd={onCardTouchEnd}
              onClick={() => {
                // 滑动态: 点 body=收菜单（原子笔记语义）;非滑动态=进编辑器。
                if (swipingId === n.id || swipingId === `fav:${n.id}`) { setSwipingId(null); return; }
                onOpen(n.id, n.title);
              }}
            >
              <div className="note-card-body" style={{ marginLeft: getLevelOverlay(n.id) * 24 }} data-ui-ver={uiVer}>
                <div className="note-title">
                  <VivoIcon def={getIconDef('doc', n.id)} size={20} />
                  {' '}
                  {n.encryption === 'aes256gcm' && (
                    <span style={{ marginRight: 4 }} aria-label="已加密" title="加密笔记 — 锁定态搜索/导出不可见">🔒</span>
                  )}
                  {n.title}
                </div>
                <div className="note-snippet">{noteSnippet(n.id) || '空笔记'}</div>
                <div className="note-meta">
                  <span>{relativeTime(n.updatedAt)}</span>
                  <span className="tag">Inbox</span>
                  {n.encryption === 'aes256gcm' && <span className="tag">加密</span>}
                </div>
              </div>
              <button
                className={`fav-btn ${favs.includes(n.id) ? 'on' : ''}`}
                onClick={(e) => { e.stopPropagation(); onToggleFav(n.id); }}
                aria-label="收藏"
              >{I.star}</button>
              <span className="note-card-chevron">{I.chev}</span>
              {swipingId === n.id && (
                <>
                  <button
                    className="swipe-delete"
                    onClick={(e) => {
                      e.stopPropagation();
                      onSoftDelete(n.id, n.title);
                      setSwipingId(null);
                      refresh();
                      showToast('已移入回收站');
                    }}
                  >删除</button>
                  <button
                    className="swipe-delete"
                    style={{ background: 'var(--text-tertiary)' }}
                    onClick={(e) => {
                      e.stopPropagation();
                      const to = n.encryption === 'aes256gcm' ? 'none' : 'aes256gcm';
                      const okk = setNoteEncryption(n.id, to);
                      setSwipingId(null);
                      refresh();
                      showToast(
                        okk
                          ? (to === 'aes256gcm' ? '🔒 已加密锁定' : '已解锁')
                          : '锁定失败（不支持或已锁定）',
                      );
                    }}
                  >{n.encryption === 'aes256gcm' ? '解锁' : '加密'}</button>
                </>
              )}
              {swipingId === `fav:${n.id}` && (
                <button
                  className="swipe-fav"
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleFav(n.id);
                    setSwipingId(null);
                    showToast(favs.includes(n.id) ? '已取消收藏' : '★ 已收藏');
                  }}
                  aria-label={favs.includes(n.id) ? '取消收藏' : '收藏'}
                >{favs.includes(n.id) ? '取消收藏' : '★ 收藏'}</button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* E27（TA 23:16）: 长按上下文菜单——底部 sheet: 删除/换图标/升降级/拖动排序 */}
      {ctxNote && (
        <div className="ctx-sheet-backdrop" onClick={() => { setCtxId(null); setCtxIconPick(false); }}>
          <div className="ctx-sheet" role="menu" aria-label="笔记操作" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">{ctxNote.title || '未命名'}</div>
            {!ctxIconPick ? (
              <>
                <button className="ctx-item" onClick={() => ctxDelete(ctxNote.id, ctxNote.title)}>🗑 删除（移入回收站）</button>
                <button className="ctx-item" onClick={() => setCtxIconPick(true)}>🎨 更换图标</button>
                <button className="ctx-item" onClick={() => ctxLevel(ctxNote.id, -1)}>↖ 升一级（减少缩进）</button>
                <button className="ctx-item" onClick={() => ctxLevel(ctxNote.id, 1)}>↘ 降一级（增加缩进）</button>
                <button className="ctx-item" onClick={() => ctxStartDrag(ctxNote.id)}>⇅ 拖动排序</button>
                <button className="ctx-item ctx-cancel" onClick={() => { setCtxId(null); setCtxIconPick(false); }}>取消</button>
              </>
            ) : (
              <>
                <div className="ctx-sheet-title">选择图标</div>
                <div className="vivo-icon-grid">
                  {ALL_ICONS.map((ic) => (
                    <button key={ic.id} className="vivo-icon-cell" title={ic.label} aria-label={ic.label}
                      onClick={() => ctxSetIcon(ctxNote.id, ic.id)}>
                      <VivoIcon def={ic} size={26} />
                    </button>
                  ))}
                </div>
                <button className="ctx-item ctx-cancel" onClick={() => setCtxIconPick(false)}>返回</button>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

// ===========================================================================
// 搜索中心 — V19 移动端页面5（自然语言 + 分区结果 + SQL 审计）
// ===========================================================================

function SearchView({ onOpen }: { onOpen: (id: string, title: string) => void }) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [history, setHistory] = useState<string[]>(() => {
    try { return JSON.parse(localStorage.getItem('aurora.searchHistory') ?? '[]'); } catch { return []; }
  });
  const [auditOpen, setAuditOpen] = useState(false);

  const doSearch = (q: string) => {
    const t = q.trim();
    if (!t) return;
    setResults(platform.searchNotes(t));
    const h = [t, ...history.filter((x) => x !== t)].slice(0, 8);
    setHistory(h);
    try { localStorage.setItem('aurora.searchHistory', JSON.stringify(h)); } catch { /* 防御 */ }
  };

  return (
    <div className="view">
      <div className="search-hero">
        <div className="search-hero-input">
          <span style={{ color: 'var(--text-tertiary)' }}>{I.search}</span>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && doSearch(query)}
            placeholder="用一句话描述要找的内容…"
            autoFocus
          />
          <button className="btn-text btn" style={{ minHeight: 36 }} onClick={() => doSearch(query)}>搜索</button>
        </div>
        <div className="search-hint">支持口语化查询，如「本周未完成的任务」「关联某项目的所有笔记」</div>

        {history.length > 0 && (
          <div className="search-history">
            {history.map((h) => (
              <button key={h} className="tag" style={{ cursor: 'pointer' }} onClick={() => { setQuery(h); doSearch(h); }}>
                {h}
              </button>
            ))}
          </div>
        )}
      </div>

      {results !== null && (
        <>
          <div className="result-section-title">笔记 · {results.length}</div>
          {results.length === 0 && <div className="empty-state"><div className="empty-text">没有找到相关内容</div></div>}
          <div className="note-list">
            {results.map((r) => (
              <div key={r.noteId} className="note-card" onClick={() => onOpen(r.noteId, r.title)}>
                <div className="note-card-body">
                  <div className="note-title">{r.title}</div>
                  <div className="note-snippet">{r.snippet}</div>
                  <div className="note-meta"><span className="search-hit-score">相关度 {(r.score * 100).toFixed(0)}%</span></div>
                </div>
              </div>
            ))}
          </div>

          <div className="result-section-title">任务 · 0</div>
          <div className="empty-state" style={{ padding: '12px 24px' }}><div className="empty-text">任务索引建设中</div></div>

          {/* V19 页面5: AI 生成 SQL 安全审计（可折叠） */}
          <button className="btn btn-text btn-block" onClick={() => setAuditOpen(!auditOpen)}>
            {auditOpen ? '收起' : '展开'}安全审计 ▾
          </button>
          {auditOpen && (
            <div className="card sql-audit">
              query = notes.search({JSON.stringify(query)}) →
              SELECT id, title, body FROM notes_idx WHERE body MATCH ? ORDER BY rank DESC LIMIT 20
            </div>
          )}
        </>
      )}
    </div>
  );
}

// ===========================================================================
// Agent — V19 移动端页面6（权限展示 + 会话状态 + 消息流）
// ===========================================================================

function AgentView({ theme }: { theme: string }) {
  void theme;
  const [msgs, setMsgs] = useState<Array<{ role: 'user' | 'agent' | 'meta'; text: string }>>([
    { role: 'meta', text: 'Agent 未授权 — 会话密钥未创建' },
  ]);
  const [input, setInput] = useState('');

  const send = () => {
    const t = input.trim();
    if (!t) return;
    setMsgs((m) => [...m,
      { role: 'user', text: t },
      { role: 'agent', text: 'Agent 尚未配置模型。前往 设置 → AI 模型 配置后即可对话（MCP 协议 · 限时会话密钥 · 操作全量审计）。' },
    ]);
    setInput('');
  };

  return (
    <div className="view">
      {/* 权限与状态 — V19 §1.3 状态标识 */}
      <div className="card">
        <div className="card-row" style={{ marginBottom: 8 }}>
          <span className="sync-badge"><span className="sync-dot off" />⏳ 模型未加载</span>
          <span style={{ flex: 1 }} />
          <span className="sync-badge">{I.lock} 私有空间</span>
        </div>
        <div className="settings-hint" style={{ padding: 0 }}>
          每次调用需二次确认 · 会话密钥限时自动过期 · 操作写入审计日志
        </div>
      </div>

      <div className="agent-msgs">
        {msgs.map((m, i) => (
          <div key={i} className={`agent-msg ${m.role}`}>{m.text}</div>
        ))}
      </div>

      <div className="agent-input-row">
        <input
          className="input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && send()}
          placeholder="向 Agent 提问…"
        />
        <button className="btn btn-primary" onClick={send}>发送</button>
      </div>
    </div>
  );
}

// ===========================================================================
// 设置 — V19 页面10 + 次级功能收纳（页面7）
// ===========================================================================

/** DK-08 §7.3: 仅 Wi-Fi 同步开关 — 初值读桥（KV 持久化在 Rust 侧），切换即时生效。 */
function WifiOnlyRow({ showToast }: { showToast: (t: string) => void }) {
  const [on, setOn] = useState(false);
  useEffect(() => {
    setOn(platform.getWifiOnly());
  }, []);
  return (
    <div className="settings-row">
      <span className="settings-row-icon">{I.wifi}</span>
      <span className="settings-row-label">
        仅 Wi-Fi 同步
        <div className="sub">蜂窝网络下暂停 P2P 同步，回到 Wi-Fi 自动恢复</div>
      </span>
      <Switch
        checked={on}
        onChange={(v) => {
          setOn(v);
          platform.setWifiOnly(v);
          showToast(v ? '仅 Wi-Fi 同步已开启' : '仅 Wi-Fi 同步已关闭');
        }}
      />
    </div>
  );
}

function SettingsView({ theme, onTheme, showToast }: {
  theme: 'light' | 'dark' | 'system';
  onTheme: (t: 'light' | 'dark' | 'system') => void;
  showToast: (t: string) => void;
}) {
  const [syncOpen, setSyncOpen] = useState(false);
  const soon = (name: string) => showToast(`${name}即将上线`);

  return (
    <div className="view">
      <div className="settings-group">
        <div className="settings-group-title">外观</div>
        <div className="settings-card">
          <div className="settings-row" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span className="settings-row-icon">{theme === 'dark' ? I.moon : I.sun}</span>
              <span className="settings-row-label">外观模式</span>
            </div>
            {/* E10b: 三段选择——浅色/深色/跟随系统（TA 14:03） */}
            <div style={{ display: 'flex', gap: 6 }}>
              {([['light', '☀️ 浅色'], ['dark', '🌙 深色'], ['system', '⚙️ 跟随系统']] as const).map(([val, label]) => (
                <button key={val} className={`nt-seg ${theme === val ? 'on' : ''}`}
                  style={{ flex: 1, height: 34, borderRadius: 8, border: 'none', cursor: 'pointer', fontSize: 12 }}
                  onClick={() => onTheme(val)}>{label}</button>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">Mirror 导出 — 活的笔记库</div>
        <div className="settings-card">
          <div className="settings-row">
            <span className="settings-row-icon">{I.note}</span>
            <span className="settings-row-label">
              Markdown 单向导出
              <div className="sub">每次保存自动写入 data/mirror/（YAML 前置）</div>
            </span>
            <span className="settings-value">自动</span>
          </div>
          <div className="settings-row">
            <span className="settings-row-icon">{I.text}</span>
            <span className="settings-row-label">
              用 Obsidian 打开该目录
              <div className="sub">即得与笔记应用同源的 Markdown 库 · 单向只读</div>
            </span>
            <span className="settings-value">v20</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">同步</div>
        <div className="settings-card">
          <div className="settings-row" onClick={() => setSyncOpen(!syncOpen)}>
            <span className="settings-row-icon">{I.sync}</span>
            <span className="settings-row-label">P2P 同步<div className="sub">iroh · NAT 穿透</div></span>
            <span className="settings-value">{syncOpen ? '收起' : '展开'}</span>
          </div>
          {syncOpen && (
            <div style={{ padding: '4px 12px 12px' }}><SyncPanel noteId="" /></div>
          )}
          <WifiOnlyRow showToast={showToast} />
          <div className="settings-row" onClick={() => soon('冲突管理')}>
            <span className="settings-row-icon">{I.note}</span>
            <span className="settings-row-label">冲突管理</span>
            <span className="settings-value">0 个冲突</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">智能</div>
        <div className="settings-card">
          <div className="settings-row" onClick={() => soon('AI 模型配置')}>
            <span className="settings-row-icon">{I.spark}</span>
            <span className="settings-row-label">AI 模型</span>
            <span className="settings-value">未配置</span>
          </div>
          <div className="settings-row" onClick={() => soon('OCR')}>
            <span className="settings-row-icon">{I.camera}</span>
            <span className="settings-row-label">OCR 能力</span>
            <span className="settings-value">未启用</span>
          </div>
          <div className="settings-row" onClick={() => soon('知识图谱')}>
            <span className="settings-row-icon">{I.today}</span>
            <span className="settings-row-label">知识图谱</span>
            <span className="settings-value">›</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">安全</div>
        <div className="settings-card">
          <div className="settings-row">
            <span className="settings-row-icon">{I.lock}</span>
            <span className="settings-row-label">工作空间<div className="sub">🔒 私有 · 端到端加密</div></span>
            <span className="settings-value">ML-KEM-768</span>
          </div>
          <div className="settings-row" onClick={() => soon('审计日志')}>
            <span className="settings-row-icon">{I.note}</span>
            <span className="settings-row-label">安全审计日志</span>
            <span className="settings-value">›</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">关于</div>
        <div className="settings-card">
          <div className="settings-row"><span className="settings-row-label">版本</span><span className="settings-value">0.13.0 · V19 UI</span></div>
          <div className="settings-row"><span className="settings-row-label">架构</span><span className="settings-value">Rust Core + React</span></div>
          <div className="settings-row"><span className="settings-row-label">引擎</span><span className="settings-value">Loro CRDT · SQLite · iroh</span></div>
        </div>
      </div>
    </div>
  );
}

// P2P 同步面板 — V19 页面8 简化版（编辑页菜单/设置共用）
function SyncPanel({ noteId }: { noteId: string }) {
  const [localAddr, setLocalAddr] = useState<string | null>(null);
  const [peerAddr, setPeerAddr] = useState('');
  const [status, setStatus] = useState('');

  const startEngine = () => {
    const addr = platform.startSyncEngine();
    setLocalAddr(addr);
    setStatus(addr ? '引擎已启动，接收循环已开启' : '引擎启动失败（可能无网络权限）');
  };

  const doSync = () => {
    if (!peerAddr.trim()) { setStatus('请输入对端地址'); return; }
    setStatus('同步中…');
    const report = platform.syncNote(peerAddr.trim(), noteId);
    if (!report) { setStatus('同步不可用（仅真机 Android）'); return; }
    setStatus(report.success ? `同步成功：↑${report.sentBytes}B ↓${report.receivedBytes}B` : `同步失败: ${report.error || '未知错误'}`);
  };

  return (
    <div className="sync-panel">
      <div className="sync-row">
        <button className="btn btn-secondary" onClick={startEngine}>启动引擎</button>
        {localAddr && <span className="sync-addr" title={localAddr}>{localAddr.slice(0, 48)}…</span>}
      </div>
      <div className="sync-row">
        <input
          className="input"
          style={{ flex: 1 }}
          value={peerAddr}
          onChange={(e) => setPeerAddr(e.target.value)}
          placeholder='粘贴对端地址 JSON {"ip":…}'
        />
        <button className="btn btn-primary" onClick={doSync}>同步</button>
      </div>
      {status && <div className="sync-status">{status}</div>}
    </div>
  );
}

// ===========================================================================
// 编辑页 — V19 移动端页面4（全屏沉浸 · 顶部返回+标题+工具菜单 · 专注模式）
// ===========================================================================

function NoteEditor({ noteId, title, isFav, onToggleFav, onClose, onDeleted, onOpenQueue, onOpenConflicts, showToast }: {
  noteId: string;
  title: string;
  isFav: boolean;
  onToggleFav: () => void;
  onClose: () => void;
  onDeleted: () => void;
  /** DK-50：编辑态菜单的同步两入口（App 层面板 state 下传） */
  onOpenQueue: () => void;
  onOpenConflicts: () => void;
  showToast: (t: string | ToastAction) => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [focusMode, setFocusMode] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);
  const [snapOpen, setSnapOpen] = useState(false);
  const [snapSaved, setSnapSaved] = useState(false);
  const [status, setStatus] = useState<'dirty' | 'saved' | null>(null);
  // 大标题可编辑（参考图布局）— overlay 存储，所有列表同步
  const [docTitle, setDocTitle] = useState(() => getTitleOverlay(noteId) || title);

  /** E6 导入导出 handler——noteExport 模块。E14（TA 18:16）: 全链 toast 反馈+防御——
   * 真机 WebView 无 DownloadListener 时 blob 静默（「无反应」根因之一）;share 双通道
   * （noteExport）+toast 保用户可感知;lazy/导出异常 catch 兜底不再静默。 */
  // E18（TA 20:24）: 导出产物内存存+查看链——toast 短暂显示[查看],点击直开内容预览
  // （「未见导出文档」根除:内容 App 内可开,不赌系统下载/share 落盘可见性）
  const [exportPreview, setExportPreview] = useState(false);
  const [lastExport, setLastExport] = useState<{ filename: string; content: string; mime: string } | null>(null);
  // E25: 保存位置——真机桥 label 反映;无桥隐藏行
  const [exportDirLabel, setExportDirLabel] = useState('');
  const e25Bridge = (window as unknown as {
    AndroidBridge?: {
      getExportDirLabel(): string;
      pickExportDir(): void;
      resetExportDir(): void;
    };
  }).AndroidBridge;
  useEffect(() => {
    if (e25Bridge) setExportDirLabel(e25Bridge.getExportDirLabel());
  }, [e25Bridge]);
  // E25: 原生导出结果回推（真文件写完/失败——toast 感知;不依赖 noteExport share 层）
  useEffect(() => {
    (window as unknown as Record<string, unknown>).__onExportDone = (ok: boolean, path: string) => {
      showToast(ok ? `已保存: ${path || '所选位置'}` : '导出失败');
    };
    return () => { delete (window as unknown as Record<string, unknown>).__onExportDone; };
  }, []);
  const pickExportDir = () => {
    if (!e25Bridge) return;
    e25Bridge.pickExportDir();
    (window as unknown as Record<string, unknown>).__onExportDirPicked = (ok: boolean) => {
      setExportDirLabel(ok ? e25Bridge.getExportDirLabel() : exportDirLabel);
      showToast(ok ? '导出位置已保存' : '已取消');
    };
  };
  const resetExportDir = () => {
    if (!e25Bridge) return;
    e25Bridge.resetExportDir();
    setExportDirLabel(e25Bridge.getExportDirLabel());
    showToast('已恢复默认位置');
  };
  const doExport = async (fmt: 'md' | 'word' | 'txt' | 'pdf') => {
    try {
      const m = await import('@aurora/ui-components');
      const t = (docTitle || title).replace(/[\\/:*?"<>|]/g, '_');
      const r = await (fmt === 'pdf' ? m.exportNotePdf(title) : m.exportNote(title, fmt));
      setStatus(r.ok ? 'saved' : null);
      if (r.ok) {
        // E25（TA 17:32）: 真文件落盘优先——有 AndroidBridge 即调原生 exportFile
        // （自定义目录 DocumentsContract 直写/默认 Download/AuroraExports/兜底系统另存面板）;
        // PDF 走 print 链无 content 不走此路。无桥（浏览器/预览）= 原 share/blob 链。
        const bridge = (window as unknown as { AndroidBridge?: { exportFile(f: string, m: string, t: string): void } }).AndroidBridge;
        if (bridge && r.content) {
          bridge.exportFile(r.filename, r.mime ?? 'text/plain', r.content);
          return; // 原生 toast+__onExportDone 回推接管反馈（noteExport share 不再叠层）
        }
        if (r.content) { setLastExport({ filename: r.filename, content: r.content, mime: r.mime ?? 'text/plain' }); showToast({ text: `已导出 ${r.filename}`, actionLabel: '查看', onAction: () => setExportPreview(true) }); }
        else showToast(`已导出 ${r.filename}`); // 桌面 print 链无内存产物
      } else showToast(`导出失败：${r.error ?? '未知错误'}`);
    } catch { showToast('导出失败（模块加载异常）'); }
  };
  const doImport = async () => {
    try {
      const m = await import('@aurora/ui-components');
      const f = await m.pickImportFile();
      if (!f) return; // 用户取消=静默
      const isMd = /\.md$|\.markdown$/i.test(f.name);
      const ok = m.importNoteText(f.text, isMd);
      showToast(ok ? `已导入 ${f.name}` : '导入失败（内容解析异常）');
    } catch { showToast('导入失败（模块加载异常）'); }
  };

  const onTitleChange = (v: string) => {
    setDocTitle(v);
    setTitleOverlay(noteId, v.trim() || title);
  };

  return (
    <div className={`editor-page ${focusMode ? 'focus' : ''}`}>
      <div className="editor-bar">
        <button className="icon-btn" onClick={onClose} aria-label="返回">{I.back}</button>
        {/* E6: status 撑中（flex:1）——⋮ 恒最右（TA 22:26 部署令） */}
        <span className={`editor-bar-status ${status ?? ''}`} style={{ flex: 1 }}>
          {status === 'dirty' ? '保存中…' : status === 'saved' ? '已保存' : ''}
        </span>
        <button className="icon-btn" onClick={() => setMenuOpen(!menuOpen)} aria-label="更多">{I.more}</button>
      </div>

      {/* 大标题区 — 参考图: 独立行 + 可编辑 + 分割线 */}
      <div className="editor-head">
        <input
          className="editor-title-input"
          value={docTitle}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="无标题"
          aria-label="笔记标题"
        />
      </div>

      {/* E27a: 删除确认独立 sheet——脱离菜单开态（真机 backdrop 触摸竞态:菜单内嵌确认链断点防御） */}
      {confirmDel && (
        <div className="ctx-sheet-backdrop" onClick={() => setConfirmDel(false)}>
          <div className="ctx-sheet" role="alertdialog" aria-label="删除确认" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">删除笔记「{docTitle || title || '未命名'}」？</div>
            <div className="ctx-item" style={{ textAlign: 'center', color: 'var(--text-tertiary)', fontSize: 12, cursor: 'default' }}>移入回收站（30 天后自动清除）</div>
            <button className="ctx-item danger" onClick={() => {
              snippetCache.delete(noteId);
              setConfirmDel(false);
              onDeleted(); // 软删除 — 内容进回收站（失败已在 softDelete 可见化）
            }}>确认删除</button>
            <button className="ctx-item ctx-cancel" onClick={() => setConfirmDel(false)}>取消</button>
          </div>
        </div>
      )}
      {menuOpen && (
        <>
          <div style={{ position: 'fixed', inset: 0, zIndex: 65 }} onClick={() => setMenuOpen(false)} />
          <div className="menu-pop">
            <button className="menu-item" onClick={() => { onToggleFav(); setMenuOpen(false); }}>
              {I.star} {isFav ? '取消收藏' : '收藏笔记'}
            </button>
            <button className="menu-item" onClick={() => { setSyncOpen(!syncOpen); setMenuOpen(false); }}>
              {I.sync} P2P 同步
            </button>
            <button className="menu-item" onClick={() => { onOpenQueue(); setMenuOpen(false); }}>
              {I.sync} 离线队列
            </button>
            <button className="menu-item" onClick={() => { onOpenConflicts(); setMenuOpen(false); }}>
              {I.sync} 冲突处理
            </button>
            <button className="menu-item" onClick={() => { setFocusMode(!focusMode); setMenuOpen(false); }}>
              {I.focus} {focusMode ? '退出专注模式' : '专注模式'}
            </button>
            {/* E6 导入导出（TA 22:33 派单）: Word/PDF/Markdown/TXT 导出 + md/txt 导入 */}
            <div className="menu-group-title">导入导出</div>
            {/* E25（TA 17:32）: 保存位置——默认 Download/AuroraExports+系统选取（SAF）;真机桥驱动 */}
            <div className="menu-item export-dir-row" role="group" aria-label="导出保存位置">
              <span className="export-dir-label">{exportDirLabel}</span>
              <button className="export-dir-btn" onClick={pickExportDir}>选择位置</button>
              <button className="export-dir-btn" onClick={resetExportDir}>默认</button>
            </div>
            <button className="menu-item" onClick={async () => { setMenuOpen(false); await doImport(); }}>
              {I.sync} 导入笔记（md/txt）
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('md'); }}>
              {I.sync} 导出 Markdown
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('word'); }}>
              {I.sync} 导出 Word
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('pdf'); }}>
              {I.sync} 导出 PDF
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('txt'); }}>
              {I.sync} 导出 TXT
            </button>
            <button className="menu-item" onClick={() => { setSnapOpen(!snapOpen); setMenuOpen(false); }}>
              {I.note} 版本历史（时间机器）
            </button>
            <button className="menu-item danger" onClick={() => { setMenuOpen(false); setConfirmDel(true); }}>
              {I.trash} 删除笔记
            </button>
          </div>
        </>
      )}

      {syncOpen && (
        <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--border)' }}>
          <SyncPanel noteId={noteId} />
        </div>
      )}

      {snapOpen && (
        <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ fontSize: 13, opacity: 0.7, lineHeight: 1.6, marginBottom: 8 }}>
            每次保存自动留版（20 版滑动窗口）· 点版本号查看存档正文
          </div>
          <button
            className="btn btn-secondary btn-block"
            onClick={() => {
              try {
                const c = platform.getNoteContent(noteId);
                platform.saveNoteSnapshot(noteId, btoa(unescape(encodeURIComponent(c))));
                setSnapSaved(true);
              } catch { setSnapSaved(false); }
            }}
          >
            {snapSaved ? '✓ 已存档（刷新列表可见）' : '手动存档当前版本'}
          </button>
          <SnapshotList noteId={noteId} refreshKey={(snapSaved ? 1 : 0) + (snapOpen ? 10 : 0)} />
        </div>
      )}

      <React.Suspense fallback={<div className="editor-loading-tip"><span className="spinner" />编辑器加载中…</div>}>
        <RichEditor
          noteId={noteId}
          platform={editorPlatform}
          mobileDock
          fallbackText={(() => { try { return platform.getNoteContent(noteId); } catch { return ''; } })()}
          onDirty={() => setStatus('dirty')}
          onSaved={() => setStatus('saved')}
        />
      </React.Suspense>

      {/* E18 导出查看 Sheet: 内容直开（html→iframe 渲染/md·txt→pre 文本） */}
      {exportPreview && lastExport && (
        <>
          <div className="sheet-mask" onClick={() => { setExportPreview(false); setLastExport(null); }} />
          <div className="sheet export-preview-sheet">
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">{lastExport.filename}</span>
              <button className="icon-btn" onClick={() => { setExportPreview(false); setLastExport(null); }} aria-label="关闭">{I.x}</button>
            </div>
            <div className="sheet-body export-preview-body">
              {/text\/html/.test(lastExport.mime) ? (
                <iframe title="导出预览" srcDoc={lastExport.content} style={{ width: '100%', height: '100%', border: 'none', borderRadius: 8, background: '#fff' }} />
              ) : (
                <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontSize: 13, lineHeight: 1.7, fontFamily: 'inherit' }}>{lastExport.content}</pre>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// ===========================================================================
// Drawer2 — 参考图3 抽屉: 标题+关闭(无logo) · 统计卡 · 分组导航 · 数字角标
// ===========================================================================

function Drawer2({ current, onNavigate, onClose, tasks, favs, trash }: {
  current: ViewId;
  onNavigate: (v: ViewId) => void;
  onClose: () => void;
  tasks: Task[];
  favs: string[];
  trash: TrashItem[];
}) {
  const notes = listNotesTitled();
  const openTasks = tasks.filter((t) => t.status !== 'done').length;
  const doneTasks = tasks.filter((t) => t.status === 'done').length;

  const item = (v: ViewId, icon: React.ReactNode, label: string, badge?: number, badgeTone?: 'brand' | 'warn') => (
    <button
      className={`d-item ${current === v ? 'active' : ''}`}
      onClick={() => onNavigate(v)}
    >
      <span className="d-item-icon">{icon}</span>
      <span className="d-item-label">{label}</span>
      {badge !== undefined && badge > 0 && (
        <span className={`d-badge ${badgeTone === 'warn' ? 'warn' : ''}`}>{badge}</span>
      )}
    </button>
  );

  return (
    <>
      <div className="drawer-mask" onClick={onClose} />
      <aside className="drawer2">
        {/* 顶部: 标题 + 关闭（无 logo — 用户要求） */}
        <div className="d-head">
          <span className="d-head-title">功能导航</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>

        {/* 统计卡 — 头像 + 三列数据（参考图1） */}
        <div className="d-profile" onClick={() => onNavigate('settings')}>
          <div className="d-avatar">A</div>
          <div className="d-profile-info">
            <div className="d-profile-name">Aurora 用户</div>
            <div className="d-profile-sub">本地优先 · 免费版</div>
          </div>
        </div>
        <div className="d-stats">
          <button className="d-stat" onClick={() => onNavigate('notes')}>
            <span className="d-stat-num">{notes.length}</span>
            <span className="d-stat-label">笔记</span>
          </button>
          <button className="d-stat" onClick={() => onNavigate('tasks')}>
            <span className="d-stat-num ok">{doneTasks}</span>
            <span className="d-stat-label">已完成</span>
          </button>
          <button className="d-stat" onClick={() => onNavigate('tasks')}>
            <span className="d-stat-num warn">{openTasks}</span>
            <span className="d-stat-label">进行中</span>
          </button>
        </div>

        <div className="d-scroll">
          <div className="d-group-title">快捷操作</div>
          {item('notes', I.notes, '全部笔记', notes.length)}
          {item('recent', I.clock, '最近浏览')}
          {item('favorites', I.star, '我的收藏', favs.length)}

          <div className="d-group-title">事务管理</div>
          {item('today', I.today, '今日视图')}
          {item('tasks', I.taskSquare, '任务中心', openTasks, 'warn')}
          {item('diary', I.book, '日记本')}

          <div className="d-group-title">更多</div>
          {item('trash', I.trash, '回收站', trash.length)}
          {item('settings', I.gear, '设置')}

          <div className="d-footer">Aurora Note · v0.14 · 本地优先</div>
        </div>
      </aside>
    </>
  );
}

// ===========================================================================
// 收藏 / 最近浏览 / 回收站 / 任务中心 / 日记本
// ===========================================================================

function FavoritesView({ favs, onOpen }: { favs: string[]; onOpen: (id: string, title: string) => void }) {
  const notes = listNotesTitled().filter((n) => favs.includes(n.id));
  return (
    <div className="view">
      {notes.length === 0 ? (
        <EmptyState text="还没有收藏的笔记。在笔记编辑页菜单中点击「收藏」即可添加。" />
      ) : (
        <div className="note-list">
          {notes.map((n) => (
            <NoteCard key={n.id} note={n} onOpen={onOpen} />
          ))}
        </div>
      )}
    </div>
  );
}

function RecentView({ recents, onOpen }: { recents: RecentItem[]; onOpen: (id: string, title: string) => void }) {
  return (
    <div className="view">
      {recents.length === 0 ? (
        <EmptyState text="最近浏览记录将出现在这里。" />
      ) : (
        <div className="note-list">
          {recents.map((r) => (
            <div key={r.id} className="note-card" onClick={() => onOpen(r.id, r.title)}>
              <div className="note-card-body">
                <div className="note-title">{r.title}</div>
                <div className="note-meta">{relativeTime(new Date(r.at).toISOString())}浏览</div>
              </div>
              <span className="note-card-chevron">›</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function TrashView({ trash, onTrash, showToast }: {
  trash: TrashItem[];
  onTrash: React.Dispatch<React.SetStateAction<TrashItem[]>>;
  showToast: (m: string) => void;
}) {
  /** S2-B 真源（2026-11-01）: 渲染序取 platform.listTrashed()（bridge/core 或 mock aurora.trash）——
   * K_TRASH 演示态与 Dto 态并集渲染（旧数据兼容）;恢复/彻底删走真桥命令。 */
  const [dtoList, setDtoList] = useState(() => platform.listTrashed());
  useEffect(() => { setDtoList(platform.listTrashed()); }, [trash.length]);
  // 合并去重（K_TRASH 旧项 + Dto 项——noteId=id 语义同源）
  const dtoIds = new Set(dtoList.map((d) => d.noteId));
  const merged: Array<{ id: string; title: string; text: string; deletedAt: number }> = [
    ...trash.filter((t) => !dtoIds.has(t.id)).map((t) => ({ id: t.id, title: t.title, text: t.text, deletedAt: t.deletedAt })),
    ...dtoList.map((d) => ({ id: d.noteId, title: d.title, text: '', deletedAt: d.deletedAtMs })),
  ];
  merged.sort((a, b) => b.deletedAt - a.deletedAt);
  // S2-B 切真桥（2026-11-01）: restore 走 platform.restoreNote——保 id/时间戳/树位
  // （createNote 新 id=演示级旧路——丢收藏/最近/树位映射）;purge 同理。
  const restore = (t: { id: string; title: string; text: string; deletedAt: number }) => {
    const ok = platform.restoreNote(t.id);
    if (ok) {
      onTrash((prev) => { const n = prev.filter((x) => x.id !== t.id); saveList(K_TRASH, n); return n; });
      setDtoList(platform.listTrashed());
      showToast('已恢复（原位）');
    } else {
      showToast('恢复失败——已在回收站外');
    }
  };
  const purge = (t: { id: string; title: string; text: string; deletedAt: number }) => {
    const ok = platform.purgeNote(t.id);
    if (ok) {
      onTrash((prev) => { const n = prev.filter((x) => x.id !== t.id); saveList(K_TRASH, n); return n; });
      setDtoList(platform.listTrashed());
      showToast('已彻底删除');
    } else {
      showToast('删除失败——不在回收站');
    }
  };
  const clearAll = () => {
    platform.purgeExpired(0); // 0=全清（core 语义 days=0 即时过期）
    onTrash(() => { saveList(K_TRASH, []); return []; });
    setDtoList(platform.listTrashed());
    showToast('回收站已清空');
  };

  return (
    <div className="view">
      {merged.length === 0 ? (
        <EmptyState text="回收站是空的。删除的笔记会在这里保留，可随时恢复。" />
      ) : (
        <>
          <button className="btn btn-text btn-block" style={{ marginBottom: 8 }} onClick={clearAll}>
            清空回收站（{merged.length}）
          </button>
          <div className="note-list">
            {merged.map((t) => (
              <div key={t.id} className="note-card">
                <div className="note-card-body">
                  <div className="note-title">{t.title}</div>
                  <div className="note-snippet">{t.text.slice(0, 50) || '（无文本内容）'}</div>
                  <div className="note-meta">删除于 {relativeTime(new Date(t.deletedAt || Date.now()).toISOString())}</div>
                </div>
                <div className="trash-actions">
                  <button className="btn btn-secondary btn-sm" onClick={() => restore(t)}>
                    {I.restore} 恢复
                  </button>
                  <button className="btn btn-text btn-sm danger-text" onClick={() => purge(t)}>删除</button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function TasksView({ tasks, onTasks, showToast, onOpenTask }: {
  tasks: Task[];
  onTasks: (ts: Task[]) => void;
  showToast: (m: string) => void;
  /** 11-08 23:10 用户令: 单击已有待办→跳转编辑（日历对应日 day 页签）。 */
  onOpenTask?: (t: Task) => void;
}) {
  const [filter, setFilter] = useState<'all' | 'open' | 'done'>('all');
  const [prioFilter, setPrioFilter] = useState<'all' | TaskPriority>('all');
  /* 11-08 23:10 用户令: 长按 550ms 删除——touchmove(滑删手势)即取消;触发后置 fired 抑制松手 click 跳转。 */
  const lp = useRef<{ timer: ReturnType<typeof setTimeout> | null; fired: boolean }>({ timer: null, fired: false });
  const open = tasks.filter((t) => t.status !== 'done');
  const done = tasks.filter((t) => t.status === 'done');
  const base = filter === 'all' ? tasks : filter === 'open' ? open : done;
  // E17: 优先级过滤+排序权重（high>normal>low>无;同级 due 近者前）
  const PRIO_W: Record<string, number> = { high: 0, normal: 1, low: 2 };
  const shown = (prioFilter === 'all' ? base : base.filter((t) => (t.priority ?? 'normal') === prioFilter))
    .slice().sort((a, b) => {
      const w = (PRIO_W[a.priority ?? 'normal'] ?? 1) - (PRIO_W[b.priority ?? 'normal'] ?? 1);
      if (w !== 0) return w;
      return (a.due ?? '9999').localeCompare(b.due ?? '9999');
    });

  const toggle = (id: string) =>
    onTasks(tasks.map((t) => t.id === id
      ? { ...t, status: t.status === 'done' ? 'next' : 'done' } : t));

  const cyclePrio = (id: string) =>
    onTasks(tasks.map((t) => {
      if (t.id !== id) return t;
      const order: TaskPriority[] = ['low', 'normal', 'high'];
      const cur = order.indexOf(t.priority ?? 'normal');
      return { ...t, priority: order[(cur + 1) % 3] };
    }));

  const remove = (id: string) => {
    onTasks(tasks.filter((t) => t.id !== id));
    showToast('任务已删除');
  };

  // E17: 日期分组（逾期/今日/明日/本周/之后/无日期）
  const todayStr = (() => { const n = new Date(); return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`; })();
  const addDays = (n: number) => { const d = new Date(); d.setDate(d.getDate() + n); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };
  const tomorrowStr = addDays(1);
  const weekEndStr = addDays(7);
  const bucketOf = (t: Task): { key: string; label: string; danger?: boolean } => {
    if (!t.due) return { key: 'nodue', label: '稍后安排' };
    if (t.due < todayStr) return { key: 'overdue', label: '已逾期', danger: true };
    if (t.due === todayStr) return { key: 'today', label: '今日' };
    if (t.due === tomorrowStr) return { key: 'tomorrow', label: '明日' };
    if (t.due <= weekEndStr) return { key: 'week', label: '本周' };
    return { key: 'later', label: '之后' };
  };
  const GROUP_ORDER = ['overdue', 'today', 'tomorrow', 'week', 'later', 'nodue'];
  const grouped = new Map<string, Task[]>();
  for (const t of shown) {
    const b = bucketOf(t).key;
    if (!grouped.has(b)) grouped.set(b, []);
    grouped.get(b)!.push(t);
  }

  // E17: 统计头（完成率环+今日量+逾期量）
  const total = tasks.length;
  const donePct = total ? Math.round((done.length / total) * 100) : 0;
  const todayCount = tasks.filter((t) => t.due === todayStr && t.status !== 'done').length;
  const overdueCount = tasks.filter((t) => t.due && t.due < todayStr && t.status !== 'done').length;
  const highOpen = open.filter((t) => (t.priority ?? 'normal') === 'high').length;

  return (
    <div className="view">
      {/* E17 统计头: 完成率+今日+逾期+高优（一屏知全局） */}
      <div className="task-stats-row">
        <div className="task-stat" style={{ flex: '0 0 auto' }}>
          <div className="task-stat-ring" style={{ background: `conic-gradient(var(--success) ${donePct * 3.6}deg, var(--bg-secondary) 0)` }}>
            <span className="task-stat-ring-num">{donePct}%</span>
          </div>
        </div>
        <div className="task-stat"><span className="task-stat-num">{todayCount}</span><span className="task-stat-label">今日</span></div>
        <div className="task-stat"><span className="task-stat-num" style={{ color: overdueCount ? 'var(--danger)' : undefined }}>{overdueCount}</span><span className="task-stat-label">逾期</span></div>
        <div className="task-stat"><span className="task-stat-num" style={{ color: highOpen ? 'var(--danger)' : undefined }}>{highOpen}</span><span className="task-stat-label">高优先</span></div>
      </div>

      <div className="chip-row">
        <button className={`chip ${filter === 'all' ? 'active' : ''}`} onClick={() => setFilter('all')}>
          全部 {tasks.length}
        </button>
        <button className={`chip ${filter === 'open' ? 'active' : ''}`} onClick={() => setFilter('open')}>
          进行中 {open.length}
        </button>
        <button className={`chip ${filter === 'done' ? 'active' : ''}`} onClick={() => setFilter('done')}>
          已完成 {done.length}
        </button>
        <span className="chip-row-sep" />
        {/* E17: 优先级过滤段 */}
        {TASK_PRIO.map((p) => (
          <button key={p.id} className={`chip ${prioFilter === p.id ? 'active' : ''}`}
            style={prioFilter === p.id ? { background: p.dot, color: '#fff', borderColor: p.dot } : undefined}
            onClick={() => setPrioFilter(prioFilter === p.id ? 'all' : p.id)}>
            {p.label} {base.filter((t) => (t.priority ?? 'normal') === p.id).length}
          </button>
        ))}
      </div>

      {shown.length === 0 ? (
        <EmptyState text="暂无任务。点右下角 + 捕获，加入今日任务。" />
      ) : (
        /* E17: 日期分组渲染（逾期/今日/明日/本周/之后/稍后——组头计数+逾期红头） */
        GROUP_ORDER.filter((k) => grouped.has(k)).map((k) => {
          const list = grouped.get(k)!;
          const b = bucketOf(list[0]);
          return (
            <div key={k} className="task-group">
              <div className={`task-group-head ${b.danger ? 'danger' : ''}`}>
                <span>{b.label}</span><span className="task-group-count">{list.length}</span>
              </div>
              <div className="card" style={{ padding: 4 }}>
                {list.map((t) => {
                  const subs = tasks.filter((st) => st.parentTaskId === t.id);
                  const subDone = subs.filter((st) => st.status === 'done').length;
                  return (
                    <div key={t.id} className="task-item"
                      /* 11-08 23:10 用户令: 单击=跳转编辑(日历对应日 day 页签),长按 550ms=删除。
                         touchmove 立即取消长按(让位滑动/滚动手势);长按触发后 fired 抑制松手 click 双动作。 */
                      onClick={() => { if (lp.current.fired) { lp.current.fired = false; return; } onOpenTask?.(t); }}
                      onTouchStart={() => { lp.current.fired = false; lp.current.timer = setTimeout(() => { lp.current.fired = true; lp.current.timer = null; remove(t.id); }, 550); }}
                      onTouchMove={() => { if (lp.current.timer) { clearTimeout(lp.current.timer); lp.current.timer = null; } }}
                      onTouchEnd={() => { if (lp.current.timer) { clearTimeout(lp.current.timer); lp.current.timer = null; } }}
                    >
                      <button
                        className={`task-check ${t.status === 'done' ? 'checked' : ''}`}
                        onClick={(e) => { e.stopPropagation(); toggle(t.id); }}
                        aria-label="完成"
                      >{I.check}</button>
                      {/* E17: 优先级色点（点击循环 low→normal→high） */}
                      <button className="task-prio-dot-btn" aria-label="优先级" title="切换优先级"
                        onClick={(e) => { e.stopPropagation(); cyclePrio(t.id); }}>
                        <span className="task-prio-dot" style={{ background: TASK_PRIO.find((p) => p.id === (t.priority ?? 'normal'))?.dot }} />
                      </button>
                      <div className="task-body">
                        <div className="task-text">{t.text}</div>
                        <div className="task-meta">
                          <span className="task-status-tag">{TASK_GROUPS.find((g) => g.id === t.status)?.label}</span>
                          {t.due && <span className={`task-due ${t.due < todayStr && t.status !== 'done' ? 'overdue' : ''}`}>{t.due}</span>}
                          {subs.length > 0 && <span className="task-sub-prog">{subDone}/{subs.length}</span>}
                          <span className="task-meta-time">{relativeTime(new Date(t.createdAt).toISOString())}</span>
                        </div>
                      </div>
                      <button className="icon-btn" onClick={(e) => { e.stopPropagation(); remove(t.id); }} aria-label="删除任务">{I.x}</button>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })
      )}

      {done.length > 0 && (
        <button
          className="btn btn-text btn-block"
          style={{ marginTop: 8 }}
          onClick={() => { onTasks(open); showToast('已清除完成任务'); }}
        >
          清除全部已完成
        </button>
      )}
    </div>
  );
}

// ===========================================================================

function DiaryView({ onOpen, showToast }: {
  onOpen: (id: string, title: string) => void;
  showToast: (m: string) => void;
}) {
  const [monthOffset, setMonthOffset] = useState(0);
  const diaries = listNotesTitled()
    .filter((n) => /日记/.test(n.title))
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));

  const now = new Date();
  const view = new Date(now.getFullYear(), now.getMonth() + monthOffset, 1);
  const y = view.getFullYear();
  const m = view.getMonth();
  const firstDay = new Date(y, m, 1).getDay(); // 0=周日
  const lead = (firstDay + 6) % 7; // 周一起始
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  const fmt = (d: number) => `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
  const dayHas = (d: number) => diaries.some((n) => n.title.includes(fmt(d)));
  const todayStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;

  const writeToday = () => {
    const title = `日记 ${todayStr}`;
    const exist = diaries.find((n) => n.title.includes(todayStr));
    if (exist) { onOpen(exist.id, exist.title); return; }
    const id = platform.createNote(title);
    if (!id) return;
    showToast('开始写今天的日记');
    onOpen(id, title);
  };

  return (
    <div className="view">
      <div className="card">
        <div className="cal-head">
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o - 1)} aria-label="上月">{I.back}</button>
          <span className="cal-title">{y} 年 {m + 1} 月</span>
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o + 1)} aria-label="下月">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m9 18 6-6-6-6"/></svg>
          </button>
        </div>
        <div className="cal-grid">
          {['一', '二', '三', '四', '五', '六', '日'].map((w) => (
            <span key={w} className="cal-week">{w}</span>
          ))}
          {Array.from({ length: lead }, (_, i) => <span key={`e${i}`} />)}
          {Array.from({ length: daysInMonth }, (_, i) => {
            const d = i + 1;
            const isToday = fmt(d) === todayStr;
            const has = dayHas(d);
            return (
              <span key={d} className={`cal-day ${isToday ? 'today' : ''} ${has ? 'has' : ''}`}>
                {d}
              </span>
            );
          })}
        </div>
        <button className="btn btn-primary btn-block" style={{ marginTop: 8 }} onClick={writeToday}>
          {I.diaryPen} {diaries.some((n) => n.title.includes(todayStr)) ? '继续写今天日记' : '写今日日记'}
        </button>
      </div>

      <div className="result-section-title" style={{ marginTop: 12 }}>全部日记 · {diaries.length}</div>
      {diaries.length === 0 ? (
        <EmptyState text="写第一篇日记，记录今天的心情与思考。" />
      ) : (
        <div className="note-list">
          {diaries.map((n) => (
            <NoteCard key={n.id} note={n} onOpen={onOpen} />
          ))}
        </div>
      )}
    </div>
  );
}

/** 通用笔记卡片（收藏/日记列表复用）。 */
export function NoteCard({ note: n, onOpen }: {
  note: NoteSummary;
  onOpen: (id: string, title: string) => void;
}) {
  return (
    <div className="note-card" onClick={() => onOpen(n.id, n.title)}>
      <div className="note-card-body">
        <div className="note-title">{n.title}</div>
        <div className="note-snippet">{noteSnippet(n.id)}</div>
        <div className="note-meta">{relativeTime(n.updatedAt)}</div>
      </div>
      <span className="note-card-chevron">›</span>
    </div>
  );
}

/** 空状态 — V19 §3.3 极简文案，无多余装饰。 */
export function EmptyState({ text }: { text: string }) {
  return (
    <div className="empty-state">
      <div className="empty-text">{text}</div>
    </div>
  );
}

function Switch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button className={`switch ${checked ? 'on' : ''}`} onClick={() => onChange(!checked)}>
      <span className="switch-knob" />
    </button>
  );
}

/** 相对时间 */
function relativeTime(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const diff = Date.now() - t;
  if (diff < 60_000) return '刚刚';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  if (diff < 172_800_000) return '昨天';
  return new Date(t).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' });
}
