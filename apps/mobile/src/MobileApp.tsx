import React, { useCallback, useEffect, useRef, useState } from 'react';
import { platform, type NoteSummary, type SearchResult } from './adapters/androidPlatform';

// 编辑器懒加载 — schema/wasm 初始化失败不拖垮整个应用（白屏防御）
const RichEditor = React.lazy(() =>
  import('./editor/RichEditor').then((m) => ({ default: m.RichEditor })),
);

// ===========================================================================
// V19 §1.2 图标 — 20px 线性简约，无渐变无填充
// ===========================================================================

const I = {
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
};

// ===========================================================================
// 全局状态
// ===========================================================================

type ViewId = 'today' | 'notes' | 'search' | 'agent' | 'settings';
const VIEW_TITLES: Record<ViewId, string> = {
  today: '今日', notes: '知识库', search: '搜索', agent: 'Agent', settings: '设置',
};

/** 任务 — GTD2.0 四分区（V19 §二页面1）。localStorage 演示级存储，
 *  Rust 侧任务表落地后切换 platform API。 */
type TaskStatus = 'next' | 'waiting' | 'plan' | 'done';
interface Task {
  id: string; text: string; status: TaskStatus;
  due?: string; noteId?: string; createdAt: number;
}
const TASK_GROUPS: Array<{ id: TaskStatus; label: string }> = [
  { id: 'next', label: '下一步行动' },
  { id: 'waiting', label: '等待' },
  { id: 'plan', label: '计划' },
  { id: 'done', label: '已完成' },
];

function loadTasks(): Task[] {
  try { return JSON.parse(localStorage.getItem('aurora.tasks') ?? '[]'); } catch { return []; }
}
function saveTasks(ts: Task[]) {
  localStorage.setItem('aurora.tasks', JSON.stringify(ts));
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
function useToast() {
  const [msg, setMsg] = useState<string | null>(null);
  const show = useCallback((t: string) => {
    setMsg(t);
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => setMsg(null), 2200);
  }, []);
  return { msg, show };
}

// ===========================================================================
// 应用骨架 — 底部 Tab（V19 §3.1）+ 悬浮捕获（永不隐藏）
// ===========================================================================

export default function App() {
  const [ready, setReady] = useState(false);
  const [fallback, setFallback] = useState(false);
  const [view, setView] = useState<ViewId>('today'); // V19: TodayView 默认启动页
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(null);
  const [captureOpen, setCaptureOpen] = useState(false);
  const [theme, setTheme] = useState<'light' | 'dark'>(() =>
    (localStorage.getItem('aurora.theme') as 'light' | 'dark') ?? 'light');
  const [tasks, setTasks] = useState<Task[]>(loadTasks);
  const { msg: toastMsg, show: showToast } = useToast();

  useEffect(() => {
    localStorage.setItem('aurora.theme', theme);
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    const ok = platform.init('/data/aurora');
    setReady(true);
    setFallback(!ok || platform.isFallback());
  }, []);

  if (!ready) return <SplashScreen />;

  const updateTasks = (ts: Task[]) => { setTasks(ts); saveTasks(ts); };

  return (
    <div className="app-shell">
      {editing ? (
        <NoteEditor
          noteId={editing.id}
          title={editing.title}
          onClose={() => setEditing(null)}
          onDeleted={() => { showToast('笔记已删除'); setEditing(null); }}
        />
      ) : (
        <>
          <main className="content">
            {fallback && <div className="settings-hint" style={{ padding: '8px 16px 0' }}>⚠️ 内存模式（数据不持久化）</div>}
            {view === 'today' && (
              <TodayView
                tasks={tasks}
                onTasks={updateTasks}
                onOpenNote={(id, title) => setEditing({ id, title })}
                showToast={showToast}
              />
            )}
            {view === 'notes' && (
              <NotesView
                onOpen={(id, title) => setEditing({ id, title })}
                showToast={showToast}
                onNewNote={() => setCaptureOpen(true)}
              />
            )}
            {view === 'search' && <SearchView onOpen={(id, title) => setEditing({ id, title })} />}
            {view === 'agent' && <AgentView theme={theme} />}
            {view === 'settings' && (
              <SettingsView theme={theme} onTheme={setTheme} showToast={showToast} />
            )}
          </main>

          {/* 悬浮捕获 — 永不隐藏 */}
          <button className="fab" onClick={() => setCaptureOpen(true)} aria-label="捕获">
            {I.plus}
          </button>

          <TabBar current={view} onChange={setView} />
        </>
      )}

      {captureOpen && (
        <CaptureSheet
          onClose={() => setCaptureOpen(false)}
          onNewNote={(title, body) => {
            const id = platform.createNote(title);
            if (body) platform.saveNoteContent(id, body);
            setCaptureOpen(false);
            setEditing({ id, title });
          }}
          onNewTask={(text, status) => {
            updateTasks([...tasks, { id: `t-${Date.now()}`, text, status, createdAt: Date.now() }]);
            setCaptureOpen(false);
            setView('today');
            showToast('已添加到今日任务');
          }}
        />
      )}

      {toastMsg && <div className="toast">{toastMsg}</div>}
    </div>
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
    { id: 'search', icon: I.search, label: '搜索' },
    { id: 'agent', icon: I.agent, label: 'Agent' },
    { id: 'settings', icon: I.gear, label: '设置' },
  ];
  return (
    <nav className="tab-bar">
      {items.map((t) => (
        <button
          key={t.id}
          className={`tab-item ${current === t.id ? 'active' : ''}`}
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

function TodayView({ tasks, onTasks, onOpenNote, showToast }: {
  tasks: Task[];
  onTasks: (ts: Task[]) => void;
  onOpenNote: (id: string, title: string) => void;
  showToast: (t: string) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({ done: true });
  const [pomoRunning, setPomoRunning] = useState(false);
  const [pomoSec, setPomoSec] = useState(25 * 60);

  // 番茄钟
  useEffect(() => {
    if (!pomoRunning) return;
    const t = setInterval(() => {
      setPomoSec((s) => {
        if (s <= 1) { setPomoRunning(false); showToast('番茄钟完成 🍅'); return 25 * 60; }
        return s - 1;
      });
    }, 1000);
    return () => clearInterval(t);
  }, [pomoRunning, showToast]);

  const today = new Date();
  const dateStr = `${today.getMonth() + 1} 月 ${today.getDate()} 日`;
  const weekDay = ['日', '一', '二', '三', '四', '五', '六'][today.getDay()];
  const doneToday = tasks.filter((t) => t.status === 'done').length;
  const openCount = tasks.filter((t) => t.status !== 'done').length;

  const notes = platform.listNotes();
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
                          const n = platform.listNotes().find((x) => x.id === t.noteId);
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

      {/* 本周日历 */}
      <div className="card">
        <h3 className="card-title">本周</h3>
        <div className="week-calendar">
          {Array.from({ length: 7 }, (_, i) => {
            const d = new Date(today);
            d.setDate(d.getDate() - d.getDay() + i + 1); // 周一起
            const isToday = d.toDateString() === today.toDateString();
            return (
              <div key={i} className={`week-day ${isToday ? 'today' : ''}`}>
                <span>{['一', '二', '三', '四', '五', '六', '日'][i]}</span>
                <span className="d">{d.getDate()}</span>
                <span className="week-dot has" />
              </div>
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

function CaptureSheet({ onClose, onNewNote, onNewTask }: {
  onClose: () => void;
  onNewNote: (title: string, body: string) => void;
  onNewTask: (text: string, status: TaskStatus) => void;
}) {
  const [mode, setMode] = useState<'text' | 'ocr' | 'voice'>('text');
  const [text, setText] = useState('');

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
                  onClick={() => onNewTask(text.trim().slice(0, 60), 'next')}
                >加入今日任务</button>
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

function NotesView({ onOpen, showToast, onNewNote }: {
  onOpen: (id: string, title: string) => void;
  showToast: (t: string) => void;
  onNewNote: () => void;
}) {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [query, setQuery] = useState('');
  const [swipingId, setSwipingId] = useState<string | null>(null);

  const refresh = useCallback(() => setNotes(platform.listNotes()), []);
  useEffect(refresh, [refresh]);

  const shown = query.trim()
    ? notes.filter((n) => n.title.includes(query.trim()))
    : notes;

  // 左滑删除（V19 移动端页面1 触屏交互）
  const cardTouch = useRef<{ x: number; y: number } | null>(null);
  const onMove = (e: React.TouchEvent, id: string) => {
    if (!cardTouch.current) return;
    const dx = e.touches[0].clientX - cardTouch.current.x;
    if (dx < -64) setSwipingId(id);
  };

  return (
    <div className="view">
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
          {shown.map((n) => (
            <div
              key={n.id}
              className={`note-card ${swipingId === n.id ? 'swiped' : ''}`}
              onTouchStart={(e) => { cardTouch.current = { x: e.touches[0].clientX, y: e.touches[0].clientY }; }}
              onTouchMove={(e) => onMove(e, n.id)}
              onClick={() => swipingId !== n.id && onOpen(n.id, n.title)}
            >
              <div className="note-card-body">
                <div className="note-title">{n.title}</div>
                <div className="note-snippet">{noteSnippet(n.id) || '空笔记'}</div>
                <div className="note-meta">
                  <span>{relativeTime(n.updatedAt)}</span>
                  <span className="tag">Inbox</span>
                </div>
              </div>
              <span className="note-card-chevron">{I.chev}</span>
              {swipingId === n.id && (
                <button
                  className="swipe-delete"
                  onClick={(e) => {
                    e.stopPropagation();
                    platform.deleteNote(n.id);
                    snippetCache.delete(n.id);
                    setSwipingId(null);
                    refresh();
                    showToast('笔记已删除');
                  }}
                >删除</button>
              )}
            </div>
          ))}
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
    localStorage.setItem('aurora.searchHistory', JSON.stringify(h));
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

function SettingsView({ theme, onTheme, showToast }: {
  theme: 'light' | 'dark';
  onTheme: (t: 'light' | 'dark') => void;
  showToast: (t: string) => void;
}) {
  const [syncOpen, setSyncOpen] = useState(false);
  const soon = (name: string) => showToast(`${name}即将上线`);

  return (
    <div className="view">
      <div className="settings-group">
        <div className="settings-group-title">外观</div>
        <div className="settings-card">
          <div className="settings-row">
            <span className="settings-row-icon">{theme === 'dark' ? I.moon : I.sun}</span>
            <span className="settings-row-label">深色模式</span>
            <Switch checked={theme === 'dark'} onChange={(v) => onTheme(v ? 'dark' : 'light')} />
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

function NoteEditor({ noteId, title, onClose, onDeleted }: {
  noteId: string;
  title: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [focusMode, setFocusMode] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);
  const [status, setStatus] = useState<'dirty' | 'saved' | null>(null);

  return (
    <div className={`editor-page ${focusMode ? 'focus' : ''}`}>
      <div className="editor-bar">
        <button className="icon-btn" onClick={onClose} aria-label="返回">{I.back}</button>
        <span className="editor-bar-title">{title}</span>
        <span className={`editor-bar-status ${status ?? ''}`}>
          {status === 'dirty' ? '保存中…' : status === 'saved' ? '已保存' : ''}
        </span>
        <button className="icon-btn" onClick={() => setMenuOpen(!menuOpen)} aria-label="更多">{I.more}</button>
      </div>

      {menuOpen && (
        <>
          <div style={{ position: 'fixed', inset: 0, zIndex: 65 }} onClick={() => setMenuOpen(false)} />
          <div className="menu-pop">
            <button className="menu-item" onClick={() => { setSyncOpen(!syncOpen); setMenuOpen(false); }}>
              {I.sync} P2P 同步
            </button>
            <button className="menu-item" onClick={() => { setFocusMode(!focusMode); setMenuOpen(false); }}>
              {I.focus} {focusMode ? '退出专注模式' : '专注模式'}
            </button>
            {confirmDel ? (
              <button className="menu-item danger" onClick={() => {
                platform.deleteNote(noteId);
                snippetCache.delete(noteId);
                onDeleted();
              }}>
                {I.trash} 确认删除
              </button>
            ) : (
              <button className="menu-item danger" onClick={() => setConfirmDel(true)}>
                {I.trash} 删除笔记
              </button>
            )}
          </div>
        </>
      )}

      {syncOpen && (
        <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--border)' }}>
          <SyncPanel noteId={noteId} />
        </div>
      )}

      <React.Suspense fallback={<div className="editor-loading-tip"><span className="spinner" />编辑器加载中…</div>}>
        <RichEditor
          noteId={noteId}
          fallbackText={(() => { try { return platform.getNoteContent(noteId); } catch { return ''; } })()}
          onDirty={() => setStatus('dirty')}
          onSaved={() => setStatus('saved')}
        />
      </React.Suspense>
    </div>
  );
}

// ===========================================================================
// 工具
// ===========================================================================

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
