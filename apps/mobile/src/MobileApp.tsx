import { useCallback, useEffect, useRef, useState } from 'react';
import { platform, type NoteSummary, type SearchResult } from './adapters/androidPlatform';

// ===========================================================================
// V15 §2.1 设计令牌（从 index.html CSS 变量读取，保持单一来源）
// ===========================================================================

// ===========================================================================
// V15 §4.1 底部 5-Tab 导航
// ===========================================================================

type TabId = 'notes' | 'ai' | 'flashcards' | 'canvas' | 'settings';

const TABS: Array<{ id: TabId; icon: string; label: string }> = [
  { id: 'notes', icon: '📝', label: '笔记' },
  { id: 'ai', icon: '✨', label: 'AI' },
  { id: 'flashcards', icon: '🎴', label: '闪卡' },
  { id: 'canvas', icon: '🗺️', label: '画布' },
  { id: 'settings', icon: '⚙️', label: '设置' },
];

// V15 §4.6 四档断点
type Breakpoint = 'compact' | 'medium' | 'expanded' | 'large';

function useBreakpoint(): Breakpoint {
  const [bp, setBp] = useState<Breakpoint>(() => calcBreakpoint());
  useEffect(() => {
    const onResize = () => setBp(calcBreakpoint());
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return bp;
}

function calcBreakpoint(): Breakpoint {
  const w = typeof window !== 'undefined' ? window.innerWidth : 360;
  if (w < 600) return 'compact';   // 手机竖屏 — 单列
  if (w < 840) return 'medium';    // 手机横屏/小平板 — 双列
  if (w < 1200) return 'expanded'; // 平板 — 三列
  return 'large';                  // 桌面 — 侧栏导航
}

// ===========================================================================
// 主应用
// ===========================================================================

export function MobileApp() {
  const [tab, setTab] = useState<TabId>('notes');
  const [theme, setTheme] = useState<'light' | 'dark'>(() =>
    (localStorage.getItem('aurora.theme') as 'light' | 'dark') ?? 'light',
  );
  const [ready, setReady] = useState(false);
  const [fallback, setFallback] = useState(false);
  const bp = useBreakpoint();

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

  // V15 §4.6: large 断点切换为侧栏导航
  if (bp === 'large') {
    return <LargeLayout tab={tab} onTab={setTab} theme={theme} onTheme={setTheme} fallback={fallback} />;
  }

  return (
    <div className="app-shell">
      {fallback && (
        <div className="fallback-banner">⚠️ 数据库不可用，已切换到内存模式（数据不会持久化）</div>
      )}
      <main className="content safe-bottom">
        {tab === 'notes' && <NotesView />}
        {tab === 'ai' && <AIView />}
        {tab === 'flashcards' && <FlashcardsView />}
        {tab === 'canvas' && <CanvasView />}
        {tab === 'settings' && <SettingsView theme={theme} onTheme={setTheme} />}
      </main>
      <BottomNav current={tab} onTab={setTab} />
    </div>
  );
}

function SplashScreen() {
  return (
    <div className="splash">
      <div className="splash-logo">Aurora</div>
      <div className="splash-hint">正在加载…</div>
    </div>
  );
}

// ===========================================================================
// V15 §4.1 底部导航（触控热区 48px）
// ===========================================================================

function BottomNav({ current, onTab }: { current: TabId; onTab: (t: TabId) => void }) {
  return (
    <nav className="bottom-nav safe-bottom">
      {TABS.map((t) => (
        <button
          key={t.id}
          className={`nav-tab ${current === t.id ? 'active' : ''}`}
          onClick={() => onTab(t.id)}
        >
          <span className="nav-icon">{t.icon}</span>
          <span className="nav-label">{t.label}</span>
        </button>
      ))}
    </nav>
  );
}

// ===========================================================================
// §4.2 笔记视图 — 顶部搜索 + 笔记卡片 + FAB + 下拉刷新/左滑删除
// ===========================================================================

function NotesView() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(null);
  const [swipingId, setSwipingId] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setNotes(platform.listNotes());
  }, []);

  useEffect(refresh, [refresh]);

  // V15 §4.5 下拉刷新手势
  const pullStart = useRef(0);
  const onTouchStart = (e: React.TouchEvent) => {
    if ((e.target as HTMLElement).closest('.note-card')) return; // 卡片触摸不走下拉
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

  // V15 §4.5 卡片左滑删除
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

  const doSearch = (q: string) => {
    setQuery(q);
    setResults(q.trim() ? platform.searchNotes(q) : null);
  };

  const doCreate = () => {
    const title = `笔记 ${new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    platform.createNote(title);
    refresh();
  };

  if (editing) {
    return <NoteEditor noteId={editing.id} title={editing.title} onClose={() => setEditing(null)} />;
  }

  return (
    <div
      className="view"
      onTouchStart={onTouchStart}
      onTouchMove={onTouchMove}
    >
      <header className="view-header">
        <h1 className="view-title">笔记</h1>
        <input
          className="search-input"
          placeholder="搜索笔记…"
          value={query}
          onChange={(e) => doSearch(e.target.value)}
        />
      </header>

      {refreshing && <div className="refresh-indicator">刷新中…</div>}

      {results !== null ? (
        <div className="note-list">
          {results.length === 0 && <EmptyState text={`没有找到“${query}”相关笔记`} />}
          {results.map((r) => (
            <div key={r.noteId} className="note-card search-hit">
              <div className="note-title">{r.title}</div>
              <div className="note-snippet">{r.snippet}</div>
              <div className="note-meta">相关度 {(r.score * 100).toFixed(0)}%</div>
            </div>
          ))}
        </div>
      ) : (
        <div className="note-list">
          {notes.length === 0 && (
            <EmptyState text="还没有笔记，点击右下角 + 创建第一篇" />
          )}
          {notes.map((n) => (
            <div
              key={n.id}
              className={`note-card ${swipingId === n.id ? 'swiped' : ''}`}
              onTouchStart={onCardTouchStart}
              onTouchMove={(e) => onCardTouchMove(e, n.id)}
              onClick={() => swipingId !== n.id && setEditing({ id: n.id, title: n.title })}
            >
              <div className="note-title">{n.title}</div>
              <div className="note-meta">
                {new Date(n.updatedAt).toLocaleString('zh-CN', { hour12: false })}
              </div>
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
      )}

      {/* V15 §4.2 FAB 悬浮按钮 — 56dp */}
      <button className="fab" onClick={doCreate} aria-label="新建笔记">
        +
      </button>
    </div>
  );
}

// ===========================================================================
// 笔记编辑器 — V19 §36.3 saveNote/getNoteContent
// ===========================================================================

function NoteEditor({ noteId, title, onClose }: { noteId: string; title: string; onClose: () => void }) {
  const [content, setContent] = useState('');
  const [saved, setSaved] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);

  useEffect(() => {
    setContent(platform.getNoteContent(noteId));
  }, [noteId]);

  const save = () => {
    platform.saveNoteContent(noteId, content);
    setSaved(true);
    setTimeout(onClose, 500);
  };

  return (
    <div className="view editor-view">
      <header className="view-header">
        <button className="header-btn" onClick={onClose}>‹ 返回</button>
        <h1 className="view-title editor-title">{title}</h1>
        <button className="header-btn" onClick={() => setSyncOpen(!syncOpen)}>同步</button>
        <button className="header-btn primary" onClick={save}>保存</button>
      </header>
      <textarea
        className="editor-textarea"
        value={content}
        onChange={(e) => setContent(e.target.value)}
        placeholder="开始写作…"
        autoFocus
      />
      {syncOpen && <SyncPanel noteId={noteId} />}
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
        : `同步失败: ${report.error || '未知错误'}`
    );
  };

  return (
    <div className="sync-panel">
      <div className="sync-row">
        <button className="header-btn" onClick={startEngine}>启动引擎</button>
        {localAddr && <span className="sync-addr" title={localAddr}>{localAddr.slice(0, 48)}…</span>}
      </div>
      <div className="sync-row">
        <input
          className="sync-input"
          value={peerAddr}
          onChange={(e) => setPeerAddr(e.target.value)}
          placeholder='粘贴对端地址 JSON {"id":"…","addrs":[…]}'
        />
        <button className="header-btn primary" onClick={doSync}>同步</button>
      </div>
      {status && <div className="sync-status">{status}</div>}
    </div>
  );
}

// ===========================================================================
// §3.8 AI 助手视图（本地 Ollama 占位对话）
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
    // 本地模型未接入时回显提示
    setTimeout(() => {
      setMessages((m) => [
        ...m,
        { role: 'ai', text: '【本地模型未接入】在设置中配置 Ollama 地址后可用。支持总结笔记、生成大纲、问答等。' },
      ]);
    }, 400);
  };

  return (
    <div className="view chat-view">
      <header className="view-header">
        <h1 className="view-title">AI 助手</h1>
      </header>
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
// §3.9 闪卡复习视图
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
      <header className="view-header">
        <h1 className="view-title">闪卡复习</h1>
        <span className="view-subtitle">{index + 1} / {cards.length}</span>
      </header>
      <div
        className={`flashcard ${flipped ? 'flipped' : ''}`}
        onClick={() => setFlipped(!flipped)}
      >
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
// §3.7 无限画布视图（SVG 简化版）
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
      <header className="view-header">
        <h1 className="view-title">无限画布</h1>
      </header>
      {nodes.length === 0 ? (
        <EmptyState text="创建笔记后，这里会展示知识图谱" />
      ) : (
        <svg className="canvas-svg" viewBox="0 0 300 260">
          {nodes.map((n, i) => (
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
      <header className="view-header">
        <h1 className="view-title">设置</h1>
      </header>
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
        <div className="settings-row"><span>版本</span><span className="settings-value">0.1.0</span></div>
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

// ===========================================================================
// 大屏布局 — V15 §4.6 large 断点侧栏导航
// ===========================================================================

function LargeLayout({
  tab, onTab, theme, onTheme, fallback,
}: {
  tab: TabId;
  onTab: (t: TabId) => void;
  theme: 'light' | 'dark';
  onTheme: (t: 'light' | 'dark') => void;
  fallback: boolean;
}) {
  return (
    <div className="large-layout">
      <aside className="large-sidebar">
        <div className="large-logo">Aurora</div>
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`large-nav-item ${tab === t.id ? 'active' : ''}`}
            onClick={() => onTab(t.id)}
          >
            <span>{t.icon}</span> {t.label}
          </button>
        ))}
      </aside>
      <main className="large-content">
        {fallback && <div className="fallback-banner">⚠️ 内存模式（数据不持久化）</div>}
        {tab === 'notes' && <NotesView />}
        {tab === 'ai' && <AIView />}
        {tab === 'flashcards' && <FlashcardsView />}
        {tab === 'canvas' && <CanvasView />}
        {tab === 'settings' && <SettingsView theme={theme} onTheme={setThemeSafe(onTheme)} />}
      </main>
    </div>
  );
}

function setThemeSafe(onTheme: (t: 'light' | 'dark') => void) {
  return onTheme;
}

function EmptyState({ text }: { text: string }) {
  return (
    <div className="empty-state">
      <div className="empty-icon">🗒️</div>
      <div className="empty-text">{text}</div>
    </div>
  );
}
