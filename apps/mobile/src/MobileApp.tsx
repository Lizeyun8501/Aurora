import React, { useCallback, useEffect, useRef, useState } from 'react';
import { platform, type NoteSummary, type SearchResult } from './adapters/androidPlatform';

// 编辑器懒加载 — schema/wasm 初始化失败不拖垮整个应用（白屏防御）
const RichEditor = React.lazy(() =>
  import('./editor/RichEditor').then((m) => ({ default: m.RichEditor })),
);

// ===========================================================================
// V12 导航 — 抽屉 + AppBar（替代底部 5-Tab）
// ===========================================================================

type ViewId = 'notes' | 'search' | 'ai' | 'flashcards' | 'canvas' | 'settings';

const NAV_ITEMS: Array<{ id: ViewId; icon: string; label: string; desc: string }> = [
  { id: 'notes', icon: '📝', label: '全部笔记', desc: '浏览与管理笔记' },
  { id: 'search', icon: '🔍', label: '搜索', desc: '全文检索（Tantivy）' },
  { id: 'ai', icon: '✨', label: 'AI 助手', desc: '总结 / 大纲 / 问答' },
  { id: 'flashcards', icon: '🎴', label: '闪卡复习', desc: '间隔重复记忆' },
  { id: 'canvas', icon: '🗺️', label: '无限画布', desc: '知识图谱视图' },
  { id: 'settings', icon: '⚙️', label: '设置', desc: '外观 / 同步 / 关于' },
];

const VIEW_TITLES: Record<ViewId, string> = {
  notes: '全部笔记',
  search: '搜索',
  ai: 'AI 助手',
  flashcards: '闪卡复习',
  canvas: '无限画布',
  settings: '设置',
};

// ===========================================================================
// 主应用
// ===========================================================================

export function MobileApp() {
  const [view, setView] = useState<ViewId>('notes');
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [theme, setTheme] = useState<'light' | 'dark'>(() =>
    (localStorage.getItem('aurora.theme') as 'light' | 'dark') ?? 'light',
  );
  const [ready, setReady] = useState(false);
  const [fallback, setFallback] = useState(false);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem('aurora.theme', theme);
  }, [theme]);

  useEffect(() => {
    const dataDir = '/data/user/0/com.aurora.note/files';
    const ok = platform.init(dataDir);
    setReady(true);
    setFallback(!ok || platform.isFallback());
  }, []);

  if (!ready) return <SplashScreen />;

  const goto = (v: ViewId) => {
    setView(v);
    setEditing(null);
    setDrawerOpen(false);
  };

  return (
    <div className="app-shell">
      {/* 编辑页全屏（无 AppBar） */}
      {editing ? (
        <NoteEditor
          noteId={editing.id}
          title={editing.title}
          onClose={() => {
            snippetCache.delete(editing.id); // 摘要缓存失效
            setEditing(null);
          }}
          onDeleted={() => setEditing(null)}
        />
      ) : (
        <>
          <AppBar
            title={VIEW_TITLES[view]}
            onMenu={() => setDrawerOpen(true)}
            onSearch={() => goto('search')}
            fallback={fallback}
          />
          <main className="content safe-bottom">
            {view === 'notes' && (
              <NotesView onOpen={(id, title) => setEditing({ id, title })} />
            )}
            {view === 'search' && (
              <SearchView onOpen={(id, title) => setEditing({ id, title })} />
            )}
            {view === 'ai' && <AIView />}
            {view === 'flashcards' && <FlashcardsView />}
            {view === 'canvas' && <CanvasView />}
            {view === 'settings' && <SettingsView theme={theme} onTheme={setTheme} />}
          </main>
        </>
      )}

      <Drawer
        open={drawerOpen}
        current={view}
        onNavigate={goto}
        onClose={() => setDrawerOpen(false)}
        theme={theme}
        onTheme={setTheme}
      />
    </div>
  );
}

// ===========================================================================
// 顶栏
// ===========================================================================

function AppBar({
  title, onMenu, onSearch, fallback,
}: {
  title: string;
  onMenu: () => void;
  onSearch: () => void;
  fallback: boolean;
}) {
  return (
    <header className="app-bar">
      <button className="icon-btn" onClick={onMenu} aria-label="打开菜单">
        <span className="menu-lines"><i /><i /><i /></span>
      </button>
      <h1 className="app-bar-title">{title}</h1>
      <button className="icon-btn" onClick={onSearch} aria-label="搜索">
        <svg viewBox="0 0 24 24" width="21" height="21" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" strokeLinecap="round" />
        </svg>
      </button>
      {fallback && <span className="fallback-dot" title="内存模式（数据不持久化）" />}
    </header>
  );
}

// ===========================================================================
// 侧边抽屉
// ===========================================================================

function Drawer({
  open, current, onNavigate, onClose, theme, onTheme,
}: {
  open: boolean;
  current: ViewId;
  onNavigate: (v: ViewId) => void;
  onClose: () => void;
  theme: 'light' | 'dark';
  onTheme: (t: 'light' | 'dark') => void;
}) {
  return (
    <>
      <div
        className={`drawer-mask ${open ? 'show' : ''}`}
        onClick={onClose}
        aria-hidden={!open}
      />
      <aside className={`drawer ${open ? 'open' : ''}`} aria-hidden={!open}>
        <div className="drawer-header">
          <div className="drawer-logo">
            <span className="drawer-logo-ring" />
            <span className="drawer-logo-core" />
          </div>
          <div className="drawer-brand">
            <div className="drawer-brand-name">Aurora Note</div>
            <div className="drawer-brand-sub">本地优先 · P2P 同步</div>
          </div>
        </div>

        <nav className="drawer-nav">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              className={`drawer-item ${current === item.id ? 'active' : ''}`}
              onClick={() => onNavigate(item.id)}
            >
              <span className="drawer-item-icon">{item.icon}</span>
              <span className="drawer-item-text">
                <span className="drawer-item-label">{item.label}</span>
                <span className="drawer-item-desc">{item.desc}</span>
              </span>
            </button>
          ))}
        </nav>

        <div className="drawer-footer">
          <button
            className="drawer-theme-btn"
            onClick={() => onTheme(theme === 'dark' ? 'light' : 'dark')}
          >
            {theme === 'dark' ? '☀️ 亮色模式' : '🌙 暗色模式'}
          </button>
          <div className="drawer-version">v0.12 · Rust Core + React</div>
        </div>
      </aside>
    </>
  );
}

// ===========================================================================
// 启动屏
// ===========================================================================

function SplashScreen() {
  return (
    <div className="splash">
      <div className="splash-logo">Aurora</div>
      <div className="splash-hint">正在加载…</div>
    </div>
  );
}

// ===========================================================================
// 笔记视图 — 卡片列表 + FAB + 左滑删除
// ===========================================================================

function NotesView({ onOpen }: { onOpen: (id: string, title: string) => void }) {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [swipingId, setSwipingId] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setNotes(platform.listNotes());
  }, []);

  useEffect(refresh, [refresh]);

  const pullStart = useRef(0);
  const onTouchStart = (e: React.TouchEvent) => {
    if ((e.target as HTMLElement).closest('.note-card')) return;
    pullStart.current = e.touches[0].clientY;
  };
  const onTouchMove = (e: React.TouchEvent) => {
    const dy = e.touches[0].clientY - pullStart.current;
    if (dy > 70 && window.scrollY === 0 && !refreshing) {
      setRefreshing(true);
      setTimeout(() => {
        refresh();
        setRefreshing(false);
      }, 400);
    }
  };

  const cardTouchStart = useRef<{ x: number; y: number } | null>(null);
  const onCardTouchStart = (e: React.TouchEvent) => {
    cardTouchStart.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };
  };
  const onCardTouchMove = (e: React.TouchEvent, id: string) => {
    if (!cardTouchStart.current) return;
    const dx = e.touches[0].clientX - cardTouchStart.current.x;
    if (dx < -60) setSwipingId(id);
  };

  const doDelete = (id: string) => {
    platform.deleteNote(id);
    setSwipingId(null);
    refresh();
  };

  const doCreate = () => {
    const title = `笔记 ${new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    const id = platform.createNote(title);
    refresh();
    // 直接进入编辑器（新笔记立即写作）
    const note = id || `new-${Date.now()}`;
    onOpen(String(note), title);
  };

  return (
    <div className="view" onTouchStart={onTouchStart} onTouchMove={onTouchMove}>
      {refreshing && <div className="refresh-indicator">刷新中…</div>}

      <div className="note-list">
        {notes.length === 0 && (
          <EmptyState text="还没有笔记
点击右下角 + 创建第一篇" />
        )}
        {notes.map((n) => (
          <div
            key={n.id}
            className={`note-card ${swipingId === n.id ? 'swiped' : ''}`}
            onTouchStart={onCardTouchStart}
            onTouchMove={(e) => onCardTouchMove(e, n.id)}
            onClick={() => swipingId !== n.id && onOpen(n.id, n.title)}
          >
            <div className="note-card-body">
              <div className="note-title">{n.title}</div>
              <div className="note-snippet">{noteSnippet(n.id)}</div>
              <div className="note-meta">{relativeTime(n.updatedAt)}</div>
            </div>
            <span className="note-card-chevron">›</span>
            {swipingId === n.id && (
              <button
                className="swipe-delete"
                onClick={(e) => { e.stopPropagation(); doDelete(n.id); }}
              >
                删除
              </button>
            )}
          </div>
        ))}
      </div>

      <button className="fab" onClick={doCreate} aria-label="新建笔记">
        <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round">
          <path d="M12 5v14M5 12h14" />
        </svg>
      </button>
    </div>
  );
}

/** 笔记摘要: 取 body 纯文本前 60 字（NoteDoc body 容器，Rust 侧维护）。 */
const snippetCache = new Map<string, string>();
function noteSnippet(id: string): string {
  const cached = snippetCache.get(id);
  if (cached !== undefined) return cached;
  let s = '';
  try {
    s = platform.getNoteContent(id).replace(/\s+/g, ' ').trim().slice(0, 60);
  } catch { /* JNI 异常兜底 */ }
  snippetCache.set(id, s);
  return s;
}

/** 相对时间（"3 分钟前"）。 */
function relativeTime(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const diff = Date.now() - t;
  if (diff < 60_000) return '刚刚';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)} 天前`;
  return new Date(t).toLocaleDateString('zh-CN');
}

// ===========================================================================
// 搜索视图
// ===========================================================================

function SearchView({ onOpen }: { onOpen: (id: string, title: string) => void }) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const doSearch = (q: string) => {
    setQuery(q);
    setResults(q.trim() ? platform.searchNotes(q) : null);
  };

  return (
    <div className="view search-view">
      <div className="search-box">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" strokeLinecap="round" />
        </svg>
        <input
          ref={inputRef}
          className="search-box-input"
          placeholder="搜索笔记标题与正文…"
          value={query}
          onChange={(e) => doSearch(e.target.value)}
        />
        {query && (
          <button className="search-clear" onClick={() => doSearch('')}>✕</button>
        )}
      </div>

      {results !== null && (
        <div className="note-list">
          {results.length === 0 && <EmptyState text={`没有找到“${query}”相关笔记`} />}
          {results.map((r) => (
            <div key={r.noteId} className="note-card search-hit" onClick={() => onOpen(r.noteId, r.title)}>
              <div className="note-card-body">
                <div className="note-title">{r.title}</div>
                <div className="note-snippet">{r.snippet}</div>
                <div className="note-meta">相关度 {(r.score * 100).toFixed(0)}%</div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ===========================================================================
// 笔记编辑页 — V12: 顶栏 + 保存状态 + 更多菜单（同步/删除）+ 工具栏
// ===========================================================================

function NoteEditor({
  noteId, title, onClose, onDeleted,
}: {
  noteId: string;
  title: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const [content, setContent] = useState('');
  const [saved, setSaved] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    setContent(platform.getNoteContent(noteId));
  }, [noteId]);

  const save = () => {
    platform.saveNoteContent(noteId, content);
    setSaved(true);
    setTimeout(onClose, 500);
  };

  const doDelete = () => {
    platform.deleteNote(noteId);
    onDeleted();
  };

  return (
    <div className="view editor-view">
      <header className="editor-bar">
        <button className="icon-btn" onClick={onClose} aria-label="返回">
          <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
            <path d="m15 18-6-6 6-6" />
          </svg>
        </button>
        <div className="editor-bar-title">
          <div className="editor-bar-name">{title}</div>
          <div className="editor-bar-status">
            {saved ? '已保存' : '自动保存已开启'}
          </div>
        </div>
        <button className="icon-btn" onClick={() => setMenuOpen(!menuOpen)} aria-label="更多">
          <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <circle cx="5" cy="12" r="1.8" /><circle cx="12" cy="12" r="1.8" /><circle cx="19" cy="12" r="1.8" />
          </svg>
        </button>
      </header>

      {menuOpen && (
        <>
          <div className="menu-mask" onClick={() => setMenuOpen(false)} />
          <div className="editor-menu">
            <button className="menu-item" onClick={() => { setSyncOpen(!syncOpen); setMenuOpen(false); }}>
              🔄 P2P 同步
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); setConfirmDelete(true); }}>
              🗑️ 删除笔记
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); save(); }}>
              💾 保存并返回
            </button>
          </div>
        </>
      )}

      <React.Suspense fallback={<div className="editor-loading">编辑器加载中…</div>}>
        <RichEditor noteId={noteId} fallbackText={content} />
      </React.Suspense>

      {syncOpen && <SyncPanel noteId={noteId} />}

      {confirmDelete && (
        <div className="dialog-mask" onClick={() => setConfirmDelete(false)}>
          <div className="dialog" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-title">删除笔记？</div>
            <div className="dialog-text">「{title}」将被永久删除，无法恢复。</div>
            <div className="dialog-actions">
              <button className="dialog-btn" onClick={() => setConfirmDelete(false)}>取消</button>
              <button className="dialog-btn danger" onClick={doDelete}>删除</button>
            </div>
          </div>
        </div>
      )}

      {saved && <div className="toast">已保存 ✓</div>}
    </div>
  );
}

// ===========================================================================
// P2P 同步面板 — V19 §31 DEV-005（iroh QUIC + NAT 穿透，端点地址 JSON 交换）
// ===========================================================================

function SyncPanel({ noteId }: { noteId: string }) {
  const [localAddr, setLocalAddr] = useState<string | null>(null);
  const [peerAddr, setPeerAddr] = useState('');
  const [status, setStatus] = useState('');

  const startEngine = () => {
    const addr = platform.startSyncEngine();
    setLocalAddr(addr);
    if (addr) {
      platform.startAcceptSync(noteId);
      setStatus('引擎已启动，接收循环已开启');
    } else {
      setStatus('引擎启动失败（可能无网络权限）');
    }
  };

  const doSync = () => {
    if (!peerAddr.trim()) {
      setStatus('请输入对端地址');
      return;
    }
    setStatus('同步中…');
    const report = platform.syncNote(peerAddr.trim(), noteId);
    if (!report) {
      setStatus('同步不可用（仅真机 Android）');
      return;
    }
    setStatus(
      report.success
        ? `同步成功：↑${report.sentBytes}B ↓${report.receivedBytes}B`
        : `同步失败: ${report.error || '未知错误'}`,
    );
  };

  return (
    <div className="sync-panel">
      <div className="sync-panel-title">P2P 同步（iroh QUIC）</div>
      <div className="sync-row">
        <button className="sync-btn" onClick={startEngine}>启动引擎</button>
        {localAddr && <span className="sync-addr" title={localAddr}>{localAddr.slice(0, 48)}…</span>}
      </div>
      <div className="sync-row">
        <input
          className="sync-input"
          value={peerAddr}
          onChange={(e) => setPeerAddr(e.target.value)}
          placeholder='粘贴对端地址 {"id":"…","addrs":[…]}'
        />
        <button className="sync-btn primary" onClick={doSync}>同步</button>
      </div>
      {status && <div className="sync-status">{status}</div>}
    </div>
  );
}

// ===========================================================================
// AI 助手视图（本地 Ollama 占位对话）
// ===========================================================================

function AIView() {
  const [messages, setMessages] = useState<Array<{ role: 'user' | 'ai'; text: string }>>([
    { role: 'ai', text: '你好！我是 Aurora AI 助手。当前设备未连接本地模型（Ollama），此为界面预览。' },
  ]);
  const [input, setInput] = useState('');

  const send = () => {
    const text = input.trim();
    if (!text) return;
    setMessages((m) => [...m, { role: 'user', text }]);
    setInput('');
    setTimeout(() => {
      setMessages((m) => [
        ...m,
        { role: 'ai', text: '【本地模型未接入】在设置中配置 Ollama 地址后可用。支持总结笔记、生成大纲、问答等。' },
      ]);
    }, 400);
  };

  return (
    <div className="view chat-view">
      <div className="chat-list">
        {messages.map((m, i) => (
          <div key={i} className={`chat-bubble ${m.role}`}>
            {m.text}
          </div>
        ))}
      </div>
      <div className="chat-input-row safe-bottom">
        <input
          className="chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && send()}
          placeholder="输入消息…"
        />
        <button className="chat-send" onClick={send}>发送</button>
      </div>
    </div>
  );
}

// ===========================================================================
// 闪卡复习视图
// ===========================================================================

function FlashcardsView() {
  const [flipped, setFlipped] = useState(false);
  const [index, setIndex] = useState(0);

  const cards = [
    { front: '间隔重复的原理是什么？', back: '基于遗忘曲线，在即将遗忘的临界点复习，以最少时间达到长期记忆。' },
    { front: '主动回忆 vs 被动复习', back: '主动回忆（自测）记忆效果显著优于重复阅读；测试本身就是学习。' },
    { front: 'Aurora 的闪卡来源', back: '从笔记块一键生成，支持 Markdown 高亮块和 AI 自动抽取。' },
  ];
  const card = cards[index % cards.length];

  return (
    <div className="view">
      <div className={`flashcard ${flipped ? 'flipped' : ''}`} onClick={() => setFlipped(!flipped)}>
        <div className="flashcard-inner">
          <div className="flashcard-face front">
            <div className="flashcard-label">问题</div>
            <div className="flashcard-text">{card.front}</div>
            <div className="flashcard-hint">点击卡片查看答案</div>
          </div>
          <div className="flashcard-face back">
            <div className="flashcard-label">答案</div>
            <div className="flashcard-text">{card.back}</div>
          </div>
        </div>
      </div>
      <div className="flashcard-actions">
        <button
          className="fc-btn forget"
          onClick={() => { setFlipped(false); setIndex((i) => (i + 1) % cards.length); }}
        >
          忘记
        </button>
        <button
          className="fc-btn know"
          onClick={() => { setFlipped(false); setIndex((i) => (i + 1) % cards.length); }}
        >
          记得
        </button>
      </div>
    </div>
  );
}

// ===========================================================================
// 无限画布视图（SVG 简化版）
// ===========================================================================

function CanvasView() {
  const [nodes] = useState(() => {
    const notes = platform.listNotes();
    return notes.slice(0, 12).map((n, i) => ({
      id: n.id,
      title: n.title,
      x: 90 + Math.cos((i / 12) * Math.PI * 2) * 110,
      y: 130 + Math.sin((i / 12) * Math.PI * 2) * 90,
    }));
  });

  return (
    <div className="view">
      {nodes.length === 0 ? (
        <EmptyState text="创建笔记后，这里会展示知识图谱" />
      ) : (
        <svg className="canvas-svg" viewBox="0 0 300 260">
          {nodes.map((n) => (
            <line
              key={`l-${n.id}`}
              x1="150" y1="130" x2={n.x + 30} y2={n.y + 12}
              stroke="var(--border)" strokeWidth="1.2"
            />
          ))}
          {nodes.map((n) => (
            <g key={n.id}>
              <rect x={n.x} y={n.y} width="60" height="24" rx="6" fill="var(--brand)" opacity="0.9" />
              <text x={n.x + 30} y={n.y + 16} textAnchor="middle" fontSize="9" fill="#fff">
                {n.title.length > 6 ? n.title.slice(0, 6) + '…' : n.title}
              </text>
            </g>
          ))}
        </svg>
      )}
    </div>
  );
}

// ===========================================================================
// 设置视图
// ===========================================================================

function SettingsView({ theme, onTheme }: { theme: 'light' | 'dark'; onTheme: (t: 'light' | 'dark') => void }) {
  return (
    <div className="view">
      <div className="settings-group">
        <div className="settings-group-title">外观</div>
        <div className="settings-row">
          <span>暗色模式</span>
          <Switch checked={theme === 'dark'} onChange={(v) => onTheme(v ? 'dark' : 'light')} />
        </div>
      </div>
      <div className="settings-group">
        <div className="settings-group-title">同步</div>
        <div className="settings-row">
          <span>端到端加密同步</span>
          <Switch checked={false} onChange={() => {}} />
        </div>
        <div className="settings-hint">需要配对设备后启用（ML-KEM-768 + AES-256-GCM）</div>
      </div>
      <div className="settings-group">
        <div className="settings-group-title">关于</div>
        <div className="settings-row"><span>版本</span><span className="settings-value">0.12.0</span></div>
        <div className="settings-row"><span>架构</span><span className="settings-value">Rust Core + React</span></div>
        <div className="settings-row"><span>引擎</span><span className="settings-value">SQLite · Tantivy · Tokio</span></div>
      </div>
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

function EmptyState({ text }: { text: string }) {
  return (
    <div className="empty-state">
      <div className="empty-icon">
        <svg viewBox="0 0 48 48" width="56" height="56" fill="none">
          <rect x="10" y="6" width="28" height="36" rx="4" stroke="currentColor" strokeWidth="2.4" opacity="0.5" />
          <path d="M17 16h14M17 23h14M17 30h9" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" opacity="0.5" />
        </svg>
      </div>
      <div className="empty-text">{text}</div>
    </div>
  );
}
