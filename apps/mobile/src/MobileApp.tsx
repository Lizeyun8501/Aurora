import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { platform, type NoteSummary, type SearchResult, type CalEvent } from './adapters/androidPlatform';
// B-02 断环: 原语层单向（I/Task 族/NoteCard/EmptyState/listNotesTitled+工具）
import {
  I, type Task, type TaskStatus, type TaskPriority, type RecentItem, type TrashItem,
  TASK_GROUPS, TASK_PRIO, NoteCard, EmptyState, listNotesTitled,
  getTitleOverlay, setTitleOverlay, noteSnippet, relativeTime,
  snippetCache, loadList, saveList, K_FAVS, K_RECENT, K_TRASH,
  getLevelOverlay, setLevelOverlay, type IconDef, type ToastAction,
  getIconDef, setIconDef, BOOK_ICONS, DOC_ICONS, loadOrder, saveOrder, applyManualOrder,
} from './components/primitives';
import { focusSummary } from './components/focusSummary';
import { type TreeNodeMeta, loadTree, saveTree } from './components/tree';
import { A05BridgeEffect } from './components/a05';
import type { SharedIntent, ShareTarget } from './components/intent';
import SyncStatusBadge from './components/SyncStatusBadge';
import QueuePanel from './components/QueuePanel';
import ConflictReviewPanel from './components/ConflictReviewPanel';
import { Solar, HolidayUtil } from 'lunar-typescript';
// B-02 切片二: 视图拆出（壳=导航/状态注入/路由;views 自持渲染）
import { AgentView } from './views/AgentView';
import { TodayView } from './views/TodayView';
import { NotesView } from './views/NotesView';
import { NoteEditor } from './views/NoteEditor';
import { CaptureSheet, TaskEditSheet, ShareReceiveSheet } from './components/sheets';
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

/** 笔记标题 overlay — 编辑页改标题时覆盖显示（Rust rename API 落地前的前端方案）。 */

/** 收藏 / 最近浏览 / 回收站 — localStorage（Rust 侧元数据表落地后迁移 platform API）。 */
/** 手动排序 overlay（VIVO 原子笔记 v4·长按拖拽排序对标）——
 * core list_notes 返回 updated_at 序（3a8545 冻结面不可动）;
 * localStorage 覆盖层存 id 序列,非命中项按 core 序垫底。
 * 同 title overlay 惯例（Rust 元数据表落地后迁移）。 */
/** 应用手动序: order 命中段按 overlay 序前置,余项按原序垫底。 */

/** 任务 — GTD2.0 四分区（V19 §二页面1）。localStorage 演示级存储，
 *  Rust 侧任务表落地后切换 platform API。 */

/** V26 M2/DK-06: 周回顾汇总 — FFI 优先, localStorage 演示级降级。 */


/** A-06: 挂起 intent 形态（Java intentJson 对齐——seq/kind/content/mime/subject/at）。 */

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


// ===========================================================================
// 捕获 & AI 液化 — V19 移动端页面2（悬浮 + 唤起底部抽屉 70%）
// ===========================================================================


// ===========================================================================
// 知识库 — V19 移动端页面3（列表 + 摘要 + 任务红点 + 左滑删除）
// ===========================================================================


/** 笔记摘要缓存（body 前 60 字）。 */

// ===========================================================================
// 笔记本文档树 — V23-I5（思源式无限级目录）
// 树元数据 localStorage（doc 节点 id = noteId, 内容复用现有笔记体系）;
// core parent_id schema 下沉 + P2P 同步树结构 → V24 路线（日记记录）
// ===========================================================================


const K_TREE = 'aurora.tree';

/** 首次进入: 自动建「我的笔记本」; 现有平铺笔记走孤儿区「入树」, 不自动迁移（时序安全）。 */
/** E21 修: 初始树立即落盘（K_TREE）——否则 tab 切换 NodeTree 重挂→loadTree() 空→
 *  initialTree 生成新 b<ts> id,图标 overlay（aurora.icon.<id>）与旧 id 失联=换图标不持久。 */



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

/** A-05: resolver 注入+跳转监听（挂载期一次性——真机通道缺失时保持提示态）。 */


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






// ===========================================================================


/** 通用笔记卡片（收藏/日记列表复用）。 */



