import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { platform, type NoteSummary, type SearchResult, type CalEvent } from './adapters/androidPlatform';
// B-02 断环: 原语层单向（I/Task 族/NoteCard/EmptyState/listNotesTitled+工具）
import {
  I, type Task, type TaskStatus, type TaskPriority, type RecentItem, type TrashItem,
  TASK_GROUPS, TASK_PRIO, NoteCard, EmptyState, listNotesTitled,
  getTitleOverlay, setTitleOverlay, noteSnippet, relativeTime,
  snippetCache, loadList, saveList, K_FAVS, K_RECENT, K_TRASH,
  getLevelOverlay, setLevelOverlay,
} from './components/primitives';
import SyncStatusBadge from './components/SyncStatusBadge';
import QueuePanel from './components/QueuePanel';
import ConflictReviewPanel from './components/ConflictReviewPanel';
import { Solar, HolidayUtil } from 'lunar-typescript';
// B-02 切片二: 视图拆出（壳=导航/状态注入/路由;views 自持渲染）
import { AgentView } from './views/AgentView';
import { SyncPanel } from './views/SyncBits';
import { SettingsView } from './views/SettingsView';
import { SearchView } from './views/SearchView';
import { FavoritesView } from './views/FavoritesView';
import { RecentView } from './views/RecentView';
import { TrashView } from './views/TrashView';
import { TasksView } from './views/TasksView';
import { DiaryView } from './views/DiaryView';
import CalendarView from './views/CalendarView';
import { SplitPane } from '@aurora/ui-components'; // A-10: 平板双栏（shared 单份——桌面同源比例/拖拽语义）
import { setBlockRefResolver, BLOCK_REF_OPEN_EVENT } from '@aurora/ui-components'; // A-05: 块引用桥
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

/** 收藏 / 最近浏览 / 回收站 — localStorage（Rust 侧元数据表落地后迁移 platform API）。 */
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

/** A-06: 挂起 intent 形态（Java intentJson 对齐——seq/kind/content/mime/subject/at）。 */
interface SharedIntent {
  seq: number;
  kind: 'text' | 'image' | 'action';
  content: string;
  mime: string;
  subject: string;
  at: number;
}

/** A-06: 落位目标（浮层可改——默认今日日记,工单「用户可改目标位置」）。 */
type ShareTarget = 'daily' | 'newNote';

/** A-06: 分享落位（今日日记=追加引用块,缺则建;新建=独立笔记）。 */
export function landShare(
  platform: { listNotes(): { id: string; title: string }[]; createNote(t: string): string | null; getNoteContent(id: string): string; saveNoteContent(id: string, c: string): number | boolean },
  target: ShareTarget,
  items: SharedIntent[],
  imageB64: string | null,
): { ok: boolean; title: string } {
  const now = new Date();
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
  const frag = items.map((it) => {
    if (it.kind === 'image') {
      return `> 🖼️ [系统分享图片] ${time} · 已存本机 ${it.content}`;
    }
    return `> 📥 [系统分享] ${time}${it.subject ? ` · ${it.subject}` : ''}\n> ${it.content.replace(/\n/g, '\n> ')}`;
  }).join('\n\n');
  let noteId: string | null = '';
  let title = '';
  if (target === 'daily') {
    const exist = platform.listNotes().find((n) => n.title.includes(today) && n.title.startsWith('日记'));
    if (exist) {
      noteId = exist.id;
      title = exist.title;
    } else {
      title = `日记 ${today}`;
      noteId = platform.createNote(title);
    }
    if (!noteId) return { ok: false, title };
    const prev = platform.getNoteContent(noteId) || '';
    platform.saveNoteContent(noteId, `${prev}${prev.endsWith('\n') || prev === '' ? '' : '\n'}\n${frag}\n`);
  } else {
    title = items[0]?.subject || `分享接收 ${today} ${time}`;
    noteId = platform.createNote(title);
    if (!noteId) return { ok: false, title };
    platform.saveNoteContent(noteId, `${frag}\n`);
  }
  return { ok: noteId !== '', title };
}

/** A-06: 分享接收浮层（内容预览+目标切换——text 型 img 预览经 data URL）。 */
function ShareReceiveSheet(props: {
  items: SharedIntent[];
  imageB64: string | null;
  onClose: () => void;
  onLand: (target: ShareTarget) => void;
}) {
  const { items, imageB64, onClose, onLand } = props;
  const [target, setTarget] = useState<ShareTarget>('daily');
  const text = items.filter((i) => i.kind === 'text').map((i) => i.content).join('\n---\n');
  const img = items.find((i) => i.kind === 'image');
  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet" role="dialog" aria-label="系统分享接收">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">📥 接收到 {items.length} 项分享</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          {text && <pre className="input" style={{ whiteSpace: 'pre-wrap', margin: '0 0 10px', maxHeight: 160, overflow: 'auto', fontSize: 13 }}>{text}</pre>}
          {img && (
            <div style={{ marginBottom: 10 }}>
              {imageB64
                ? <img src={imageB64} alt="分享图片预览" style={{ maxWidth: '100%', borderRadius: 8, border: '1px solid rgba(0,0,0,0.1)' }} />
                : <div style={{ fontSize: 12, opacity: 0.7 }}>🖼️ 图片已保存（{img.content}）</div>}
            </div>
          )}
          <div className="capture-modes" role="radiogroup" aria-label="落位目标">
            <button className={`capture-mode ${target === 'daily' ? 'active' : ''}`} role="radio" aria-checked={target === 'daily'} onClick={() => setTarget('daily')}>
              📔<span>今日日记（默认）</span>
            </button>
            <button className={`capture-mode ${target === 'newNote' ? 'active' : ''}`} role="radio" aria-checked={target === 'newNote'} onClick={() => setTarget('newNote')}>
              📄<span>新建笔记</span>
            </button>
          </div>
          <button className="btn btn-primary btn-block" onClick={() => onLand(target)}>落位</button>
          <button className="btn btn-secondary btn-block" style={{ marginTop: 8 }} onClick={onClose}>暂不处理</button>
        </div>
      </div>
    </>
  );
}

function loadTasks(): Task[] {
  try { return JSON.parse(localStorage.getItem('aurora.tasks') ?? '[]'); } catch { return []; }
}
function saveTasks(ts: Task[]) {
  try { localStorage.setItem('aurora.tasks', JSON.stringify(ts)); } catch { /* 配额满/隐私模式 */ }
  // A-08 双写（铁律 6 三段式新增段）: 移动 localStorage 权威不动,core 镜像
  // 供桌面日历同源消费（CalendarVm tasks 面;切读=下卡）。失败 console.warn
  // 非静默（Java 壳空串约定——F12 可见,真机 chrome inspect 落 logcat）。
  try {
    const b = (window as { AndroidBridge?: { taskSync?: (j: string) => string } }).AndroidBridge;
    if (b?.taskSync) {
      const payload = JSON.stringify(ts.map((t) => ({
        id: t.id, text: t.text, status: t.status, due: t.due ?? null,
        note_id: t.noteId ?? null, created_at: t.createdAt,
        priority: t.priority ?? null,
        ext: { estimate_minutes: t.estimateMinutes ?? null, actual_minutes: t.actualMinutes ?? null, parent_task_id: t.parentTaskId ?? null, progress: t.progress ?? null },
      })));
      const r = b.taskSync(payload);
      if (!r) console.warn('[A-08] taskSync core 镜像失败（双写降级——localStorage 权威不受影响）');
    }
  } catch (e) { console.warn('[A-08] taskSync 异常', e); }
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
  // E28c（TA 15:44）: 按压反馈统一——:active 残留（E26④ 清后无反馈=「混乱」观感）的
  // JS 层正解: pointerdown 加 .pressing;pointerup/cancel 全局兜底移除——touch/mouse
  // 统一+保证释放（不依赖浏览器 :active 生命周期）
  useEffect(() => {
    const down = (e: PointerEvent) => {
      const b = (e.target as HTMLElement | null)?.closest?.('button');
      if (b) b.classList.add('pressing');
    };
    const clearAll = () => document.querySelectorAll('button.pressing').forEach((b) => b.classList.remove('pressing'));
    document.addEventListener('pointerdown', down);
    document.addEventListener('pointerup', clearAll);
    document.addEventListener('pointercancel', clearAll);
    return () => {
      document.removeEventListener('pointerdown', down);
      document.removeEventListener('pointerup', clearAll);
      document.removeEventListener('pointercancel', clearAll);
    };
  }, []);
  useKbdViewport(); // DK-05M：键盘视口感知（--vvh/--kbd-inset）
  const [ready, setReady] = useState(false);
  const [fallback, setFallback] = useState(false);
  const [view, setView] = useState<ViewId>('today'); // V19: TodayView 默认启动页
  /** 2026-10-28 周条→日历联动：待跳转选中日期（YYYY-MM-DD），CalendarView 挂载时消费 */
  const [calendarInit, setCalendarInit] = useState<string | null>(null);
  /** 11-09 07:31 用户令: 待办单击→编辑弹窗（TaskEditSheet）。 */
  const [editTask, setEditTask] = useState<Task | null>(null);
  const [captureOpen, setCaptureOpen] = useState(false);
  const [captureDue, setCaptureDue] = useState<string | undefined>(undefined); // 日历入口预置日期
  // ── A-06: 系统分享接收 + URL Scheme ──────────────────────────────
  /** 挂起分享队列（text/image 型——浮层确认落位;action 型即时路由）。 */
  const [shareInbox, setShareInbox] = useState<SharedIntent[]>([]);
  const [shareImageB64, setShareImageB64] = useState<string | null>(null);
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

  /* A-06: intent 消费（拉模式——三触发: 挂载/resume(visibilitychange)/Java 推事件）。
   * action 型即时路由（capture/today/task-done）;text/image 型入 shareInbox 浮层。
   * task-done 走既有 updateTasks→saveTasks 链（localStorage 权威+taskSync core 镜像
   * →投影=WritePath 语义——工单禁直改 DB 纪律）。 */
  const consumeA06Intents = () => {
    try {
      const b = (window as unknown as { AndroidBridge?: { consumePendingIntents?(): string } }).AndroidBridge;
      if (!b?.consumePendingIntents) return;
      const arr = JSON.parse(b.consumePendingIntents() || '[]') as SharedIntent[];
      const pending: SharedIntent[] = [];
      for (const it of arr) {
        if (it.kind === 'action') {
          const u = it.content;
          if (u.startsWith('aurora://capture')) { setCaptureOpen(true); setView('today'); }
          else if (u.startsWith('aurora://today')) { setView('today'); }
          else if (u.startsWith('aurora://task-done')) {
            const m = /taskId=([^&]+)/.exec(u);
            const tid = m ? decodeURIComponent(m[1]) : null;
            if (tid) {
              const t = tasks.find((x) => x.id === tid);
              if (t && t.status !== 'done') {
                updateTasks(tasks.map((x) => (x.id === tid ? { ...x, status: 'done' as const } : x)));
                showToast('通知操作:任务已完成（经保存链回写）');
              } else {
                showToast('任务已是完成态（或已不存在）');
              }
            }
            // 双通道刷新小组件（DoD <1s——任务态变更即时推）
            (window as unknown as { AndroidBridge?: { refreshWidget?(): void } }).AndroidBridge?.refreshWidget?.();
          }
        } else {
          pending.push(it);
        }
      }
      if (pending.length > 0) {
        setShareInbox(pending);
        const img = pending.find((x) => x.kind === 'image');
        if (img) {
          const bb = (window as unknown as { AndroidBridge?: { readSharedImage?(p: string): string } }).AndroidBridge;
          setShareImageB64(bb?.readSharedImage ? `data:image/png;base64,${bb.readSharedImage(img.content)}` : null);
        } else {
          setShareImageB64(null);
        }
      }
    } catch (e) { console.warn('[A-06] intent 消费异常', e); }
  };
  useEffect(() => {
    consumeA06Intents();
    const onVis = () => { if (document.visibilityState === 'visible') consumeA06Intents(); };
    const onEvt = () => consumeA06Intents();
    window.addEventListener('visibilitychange', onVis);
    window.addEventListener('aurora:intent-received', onEvt);
    return () => {
      window.removeEventListener('visibilitychange', onVis);
      window.removeEventListener('aurora:intent-received', onEvt);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  });

  /** 打开笔记 — 记录最近浏览。 */
  /* A-04: favorites→kind=note 书签迁移（幂等标记;**双写一版**——favorites
     localStorage 原链零动（切读=下卡）,书签域独立读 core——数据零丢失 DoD。 */
  const [bookmarks, setBookmarks] = useState<Array<{
    bookmark: { id: string; kind: 'note' | 'folder' | 'search'; target_id: string | null; query_json: string | null; title: string; sort_order: number };
    stale: boolean;
  }>>([]);
  const refreshBookmarks = useCallback(() => {
    try {
      const raw = (platform as unknown as { bookmarkList?: () => string }).bookmarkList?.();
      if (raw) setBookmarks(JSON.parse(raw) as typeof bookmarks);
    } catch { /* 桥未挂/解析失败——书签区空态（非静默丢: UI 显式空） */ }
  }, []);
  useEffect(() => {
    (async () => {
      try {
        const migrated = localStorage.getItem('aurora.bmMigrated');
        const favs = JSON.parse(localStorage.getItem(K_FAVS) ?? '[]') as string[];
        if (!migrated && favs.length > 0) {
          // titles 需查: favs 是 id 列表——标题从 tree/notes 缓存尽力（缺失用 id）
          for (const id of favs) {
            const bm = {
              id: `bm-mig-${id}`, workspace_id: null, kind: 'note',
              target_id: id, query_json: null,
              title: id, sort_order: -1, created_at: Date.now(),
            };
            (platform as unknown as { bookmarkAdd?: (j: string) => string }).bookmarkAdd?.(JSON.stringify(bm));
          }
          localStorage.setItem('aurora.bmMigrated', '1');
        }
      } catch { /* 隐私模式/配额——迁移降级（favorites 原数据仍在） */ }
      refreshBookmarks();
    })();
  }, [refreshBookmarks]);

  /* B-01: Agent 设置引导跳转（AgentView needSetup 卡→goto('settings')）。
   * drawer 已持有 goto——window 事件面复用（跨组件层导航,零 prop 穿透）。 */
  useEffect(() => {
    const onGoto = (e: Event) => {
      const v = (e as CustomEvent<{ view: string }>).detail?.view;
      const known = ['today', 'notes', 'search', 'calendar', 'agent', 'settings', 'favorites', 'recent', 'tasks', 'diary', 'trash'] as const;
      if (v && (known as readonly string[]).includes(v)) gotoRef.current?.(v as (typeof known)[number]);
    };
    window.addEventListener('aurora:goto', onGoto);
    return () => window.removeEventListener('aurora:goto', onGoto);
  }, []);
  const gotoRef = useRef<((v: ViewId) => void) | null>(null);

  /* A-04: search 书签跳转串（Drawer2 onSearchJump→SearchView 预填）。 */
  const [bmSearchJump, setBmSearchJump] = useState<string | undefined>(undefined);
  /* A-10: 平板双栏——宽 ≥768 启用（iPad 尺寸类;旋转=resize 不卸载组件,
     editing state 与编辑器光标跨旋转保持——DoD）。 */
  const [isWide, setIsWide] = useState(() => window.innerWidth >= 768);
  useEffect(() => {
    const mq = window.matchMedia('(min-width: 768px)');
    const onChange = () => setIsWide(mq.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);
  /* A-10: 分栏状态持久化——选中笔记 id（localStorage;栏宽=SplitPane storageKey
     自持）。重启/旋转恢复上下文。 */
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(() => {
    try {
      const saved = localStorage.getItem('aurora.tabletSelection');
      if (saved) {
        const { id, title } = JSON.parse(saved) as { id: string; title: string };
        if (id) return { id, title };
      }
    } catch { /* 隐私模式降级 */ }
    return null;
  });
  useEffect(() => {
    try {
      if (editing) localStorage.setItem('aurora.tabletSelection', JSON.stringify(editing));
      else localStorage.removeItem('aurora.tabletSelection');
    } catch { /* 配额满降级 */ }
  }, [editing]);

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
    // E28a（TA 15:44 真根因）: ⋮ 删除路径摘树节点——旧链只删 core 笔记+回收站,
    // aurora.tree 残留节点→树形模式仍渲染该行（「显示删除实际未删除」观感根因）
    try {
      const t = JSON.parse(localStorage.getItem('aurora.tree') ?? '[]') as TreeNodeMeta[];
      const next = t.filter((x) => x.id !== id);
      if (next.length !== t.length) localStorage.setItem('aurora.tree', JSON.stringify(next));
    } catch { /* 防御 */ }
    snippetCache.delete(id);
  };

  const goto = (v: ViewId) => { setView(v); setDrawerOpen(false); };
  useEffect(() => { gotoRef.current = goto; });

  /* A-10: 平板双栏渲染——wide+editing 时主框架保留（TabBar/AppBar 不退场,
     notes 视图 content 换 SplitPane: 左列表+右编辑器;NoteEditor 组件同 key
     双栏右格/单栏全屏两形态切换不重挂——光标与组件态跨旋转保持（DoD）。
     非 notes 视图/无选中走原单栏链（零行为变）。 */
  const renderEditor = (onClose: () => void) => (
    <NoteEditor
      noteId={editing!.id}
      title={editing!.title}
      isFav={favs.includes(editing!.id)}
      onToggleFav={() => toggleFav(editing!.id)}
      onClose={onClose}
      onDeleted={() => { softDelete(editing!.id, editing!.title); showToast('已移入回收站'); setEditing(null); }}
      onOpenQueue={() => setQueueOpen(true)}
      onOpenConflicts={() => setConflictOpen(true)}
      showToast={showToast}
      onOpenNote={(id, t) => { snippetCache.delete(editing!.id); setEditing({ id, title: t }); }}
    />
  );
  const tabletSplit =
    isWide && editing && view === 'notes' ? (
      <SplitPane
        storageKey="aurora.tabletSplit"
        initialSize={38}
        left={
          <NotesView
            onOpen={openNote}
            showToast={showToast}
            onNewNote={() => setCaptureOpen(true)}
            onSoftDelete={softDelete}
            favs={favs}
            onToggleFav={toggleFav}
            setNoteEncryption={(id, level) => platform.setNoteEncryption(id, level)}
          />
        }
        right={renderEditor(() => { snippetCache.delete(editing!.id); setEditing(null); })}
      />
    ) : null;

  return (
    <div className="app-shell">
      {editing && !tabletSplit ? (
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
          onOpenNote={(id, t) => { snippetCache.delete(editing!.id); setEditing({ id, title: t }); }}
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
            {view === 'notes' && (tabletSplit ?? (
              <NotesView
                onOpen={openNote}
                showToast={showToast}
                onNewNote={() => setCaptureOpen(true)}
                onSoftDelete={softDelete}
                favs={favs}
                onToggleFav={toggleFav}
                setNoteEncryption={(id, level) => platform.setNoteEncryption(id, level)}
              />
            ))}
            {view === 'search' && <SearchView onOpen={openNote} initialQuery={bmSearchJump} />}
            {view === 'calendar' && (
              <CalendarView
                tasks={tasks}
                events={events}
                onUpsertEvent={(ev) => { platform.upsertEvent(ev); refreshEvents(); platform.refreshReminders(); }}
                onDeleteEvent={(id) => { platform.deleteEvent(id); refreshEvents(); platform.refreshReminders(); }}
                onOpenNote={openNote}
                onNewTask={(due) => { setCaptureDue(due); setCaptureOpen(true); }}
                initialSelected={calendarInit ?? undefined}
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
                /* 11-09 07:31 用户令: 单击已有待办→弹出可编辑待办（非跳日历）。 */
                onOpenTask={(t) => setEditTask(t)} />
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
              bookmarks={bookmarks}
              onBookmarkRemove={(id) => {
                (platform as unknown as { bookmarkRemove?: (i: string) => string }).bookmarkRemove?.(id);
                refreshBookmarks();
              }}
              onSearchJump={(raw) => {
                setBmSearchJump(raw);
                goto('search');
                setDrawerOpen(false);
              }}
              onOpenBookmark={(id, title) => {
                openNote(id, title);
                setDrawerOpen(false);
              }}
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

      {/* 11-09 07:31 用户令: 待办单击→编辑弹窗。 */}
      {editTask && (
        <TaskEditSheet
          task={editTask}
          onClose={() => setEditTask(null)}
          onSave={(t) => { updateTasks(tasks.map((x) => (x.id === t.id ? t : x))); showToast('待办已更新'); }}
          onDelete={(id) => { updateTasks(tasks.filter((x) => x.id !== id)); showToast('待办已删除'); }}
        />
      )}

      {/* A-06: 系统分享接收浮层（目标可改——默认今日日记;确认落位+小组件刷新） */}
      {shareInbox.length > 0 && (
        <ShareReceiveSheet
          items={shareInbox}
          imageB64={shareImageB64}
          onClose={() => { setShareInbox([]); setShareImageB64(null); }}
          onLand={(target) => {
            const r = landShare(platform, target, shareInbox, shareImageB64);
            setShareInbox([]);
            setShareImageB64(null);
            showToast(r.ok ? `已落位: ${r.title}` : '落位失败（存储不可写）');
            (window as unknown as { AndroidBridge?: { refreshWidget?(): void } }).AndroidBridge?.refreshWidget?.();
          }}
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
  /** E28b（TA 15:44）: 重命名——doc 双写（title overlay+树 meta）;book 树 meta。
   * overlay 优先显示（displayTitle 惯例）——core rename API 落地前的前端方案（同标题 overlay 注释）。 */
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameText, setRenameText] = useState('');
  const startRename = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n) return;
    setCtxId(null);
    setRenaming(id);
    setRenameText(displayTitle(n));
  };
  const commitRename = () => {
    if (!renaming) return;
    const name = renameText.trim();
    const n = tree.find((t) => t.id === renaming);
    if (!n || !name) { setRenaming(null); return; }
    if (n.kind === 'doc') setTitleOverlay(n.id, name);
    persist(tree.map((t) => (t.id === renaming ? { ...t, title: name } : t)));
    setRenaming(null);
    showToast('已重命名');
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

  const childrenOf = (pid: string | null) => tree.filter((t) => t.parentId === pid && !zombieIds.has(t.id));
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

  // E28a: 僵尸 doc 过滤——platform.listNotes() 实查（props notes state 滞后一拍会误杀
  // createChild 新建节点——dbg 实证;实查 mock=localStorage 真机=桥,幂等且实时）
  let liveIds: Set<string>;
  try { liveIds = new Set(platform.listNotes().map((n) => n.note_id)); } catch { liveIds = new Set(); }
  const zombieIds = new Set(tree.filter((t) => t.kind === 'doc' && !liveIds.has(t.id)).map((t) => t.id));
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
            <button className="ctx-item" onClick={() => startRename(ctxNode.id)}>✏️ 重命名</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); levelUp(ctxNode.id); }}>↖ 升一级</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); levelDown(ctxNode.id); }}>↘ 降一级</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); setTreeDrag(ctxNode.id); showToast('拖到目标行松手（换位置/换层级）'); }}>⇅ 拖动排序</button>
            <button className="ctx-item ctx-cancel" onClick={() => setCtxId(null)}>取消</button>
          </div>
        </div>
      )}

      {/* E28b: 重命名 sheet——内联输入+确认 */}
      {renaming && (
        <div className="ctx-sheet-backdrop" onClick={() => setRenaming(null)}>
          <div className="ctx-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">重命名</div>
            <input
              className="rename-input" autoFocus value={renameText}
              onChange={(e) => setRenameText(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') commitRename(); }}
              aria-label="新名称" placeholder="输入新名称"
            />
            <button className="ctx-item" onClick={commitRename}>确认</button>
            <button className="ctx-item ctx-cancel" onClick={() => setRenaming(null)}>取消</button>
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


// ===========================================================================
// Agent — V19 移动端页面6（权限展示 + 会话状态 + 消息流）
// ===========================================================================




// P2P 同步面板 — V19 页面8 简化版（编辑页菜单/设置共用）

// ===========================================================================
// 编辑页 — V19 移动端页面4（全屏沉浸 · 顶部返回+标题+工具菜单 · 专注模式）
// ===========================================================================

/** A-05: 块引用/面包屑移动桥面（真机 AndroidBridge JSON 通道;无桥降级——
 * 嵌入卡显式提示态、面包屑不渲染,与桌面 mock 模式语义对齐）。 */
const a05FetchBreadcrumb = async (noteId: string): Promise<{ id: string; title: string }[]> => {
  try {
    const b = (window as unknown as { AndroidBridge?: { breadcrumb?(id: string): string } }).AndroidBridge;
    if (!b?.breadcrumb) return [];
    return JSON.parse(b.breadcrumb(noteId)) as { id: string; title: string }[];
  } catch {
    return [];
  }
};

/** A-05: resolver 注入+跳转监听（挂载期一次性——真机通道缺失时保持提示态）。 */
function A05BridgeEffect(props: { noteId: string; onOpenNote: (noteId: string, title: string) => void }): null {
  const { onOpenNote } = props;
  useEffect(() => {
    setBlockRefResolver(async (nid, anchor) => {
      try {
        const b = (window as unknown as { AndroidBridge?: { resolveBlockRef?(id: string, a: string): string } }).AndroidBridge;
        if (!b?.resolveBlockRef) return { status: 'not_found' } as const;
        return JSON.parse(b.resolveBlockRef(nid, anchor ?? '')) as import('@aurora/ui-components').BlockRefData;
      } catch {
        return { status: 'not_found' } as const;
      }
    });
    const onOpen = (e: Event): void => {
      const d = (e as CustomEvent<{ noteId: string; anchor?: string | null }>).detail;
      if (d?.noteId) onOpenNote(d.noteId, d.noteId);
    };
    window.addEventListener(BLOCK_REF_OPEN_EVENT, onOpen);
    return () => window.removeEventListener(BLOCK_REF_OPEN_EVENT, onOpen);
  }, [onOpenNote]);
  return null;
}

function NoteEditor({ noteId, title, isFav, onToggleFav, onClose, onDeleted, onOpenQueue, onOpenConflicts, showToast, onOpenNote }: {
  noteId: string;
  title: string;
  isFav: boolean;
  onToggleFav: () => void;
  onClose: () => void;
  onDeleted: () => void;
  /** A-05: 块引用卡「跳转源块」→ 父级编辑态切笔记。 */
  onOpenNote: (noteId: string, title: string) => void;
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

      {/* A-05: 块引用 resolver 注入 + 跳转桥（真机 AndroidBridge;无桥=提示态不空白） */}
      {/* eslint-disable-next-line react-hooks/rules-of-hooks -- 模块级注入非 hook 消费 */}
      <A05BridgeEffect noteId={noteId} onOpenNote={onOpenNote} />
      <React.Suspense fallback={<div className="editor-loading-tip"><span className="spinner" />编辑器加载中…</div>}>
        <RichEditor
          noteId={noteId}
          platform={editorPlatform}
          mobileDock
          sourceModeToggle
          fetchBreadcrumb={a05FetchBreadcrumb}
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

function Drawer2({ current, onNavigate, onClose, tasks, favs, trash, bookmarks, onBookmarkRemove, onSearchJump, onOpenBookmark }: {
  current: ViewId;
  onNavigate: (v: ViewId) => void;
  onClose: () => void;
  tasks: Task[];
  favs: string[];
  trash: TrashItem[];
  bookmarks: Array<{ bookmark: { id: string; kind: 'note' | 'folder' | 'search'; target_id: string | null; query_json: string | null; title: string; sort_order: number }; stale: boolean }>;
  onBookmarkRemove: (id: string) => void;
  onSearchJump?: (raw: string) => void;
  onOpenBookmark?: (id: string, title: string) => void;}) {
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
          {/* A-04: 书签区（三类+死链显态+移除——移动抽屉面;点击跳笔记/搜索） */}
          <div className="d-group-title">书签</div>
          {bookmarks.length === 0 && <div style={{ padding: '4px 16px', fontSize: 12, color: 'var(--muted)' }}>暂无书签</div>}
          {bookmarks.map((v) => (
            <div key={v.bookmark.id} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
              <button
                className="d-item"
                style={{ flex: 1, opacity: v.stale ? 0.55 : 1, cursor: v.stale ? 'not-allowed' : 'pointer' }}
                onClick={() => {
                  if (v.stale) return;
                  if (v.bookmark.kind === 'search') {
                    try {
                      const q = JSON.parse(v.bookmark.query_json ?? '{}') as { raw?: string };
                      if (q.raw) onSearchJump?.(q.raw);
                    } catch { /* 损坏 query 跳过 */ }
                  } else if (v.bookmark.target_id) {
                    onOpenBookmark?.(v.bookmark.target_id, v.bookmark.title);
                  }
                }}
              >
                <span>{v.bookmark.kind === 'search' ? '🔍' : v.bookmark.kind === 'folder' ? '📚' : '📄'}</span>
                <span style={{ flex: 1, textAlign: 'left', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {v.stale ? `${v.bookmark.title}（已失效）` : v.bookmark.title}
                </span>
              </button>
              <button className="icon-btn" onClick={() => onBookmarkRemove(v.bookmark.id)} aria-label={`移除书签 ${v.bookmark.title}`}>{I.x}</button>
            </div>
          ))}

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




/** 11-09 07:31 用户令: 待办单击→可编辑弹窗（非跳日历）——文本/截止日期/优先级/状态+保存/删除。
    复用 sheet/task-prio-seg 现成样式（同 CaptureSheet 观感体系）。 */
function TaskEditSheet({ task, onClose, onSave, onDelete }: {
  task: Task;
  onClose: () => void;
  onSave: (t: Task) => void;
  onDelete: (id: string) => void;
}) {
  const [text, setText] = useState(task.text);
  const [due, setDue] = useState(task.due ?? '');
  const [prio, setPrio] = useState<TaskPriority>(task.priority ?? 'normal');
  const [status, setStatus] = useState<TaskStatus>(task.status);
  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">编辑待办</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          <textarea className="input" rows={3} value={text} onChange={(e) => setText(e.target.value)} autoFocus />
          <div className="task-prio-row" role="radiogroup" aria-label="优先级">
            {TASK_PRIO.map((p) => (
              <button key={p.id} className={`task-prio-seg ${prio === p.id ? 'on' : ''}`} role="radio" aria-checked={prio === p.id}
                onClick={() => setPrio(p.id)}>
                <span className="task-prio-dot" style={{ background: p.dot }} />{p.label}
              </button>
            ))}
          </div>
          <div className="task-prio-row" role="radiogroup" aria-label="状态">
            {TASK_GROUPS.map((g) => (
              <button key={g.id} className={`task-prio-seg ${status === g.id ? 'on' : ''}`} role="radio" aria-checked={status === g.id}
                onClick={() => setStatus(g.id)}>{g.label}</button>
            ))}
          </div>
          <label style={{ display: 'flex', alignItems: 'center', gap: 10, margin: '10px 0 4px', fontSize: 13, color: 'var(--text-secondary)' }}>
            <span>截止日期</span>
            <input type="date" className="input" style={{ flex: 1 }} value={due} onChange={(e) => setDue(e.target.value)} />
          </label>
          <div className="capture-actions">
            <button className="btn btn-text" onClick={() => { onDelete(task.id); onClose(); }}>删除</button>
            <button className="btn btn-primary" disabled={!text.trim()} style={{ opacity: text.trim() ? 1 : 0.5 }}
              onClick={() => { onSave({ ...task, text: text.trim(), due: due || undefined, priority: prio, status }); onClose(); }}>保存</button>
          </div>
        </div>
      </div>
    </>
  );
}


// ===========================================================================


/** 通用笔记卡片（收藏/日记列表复用）。 */



