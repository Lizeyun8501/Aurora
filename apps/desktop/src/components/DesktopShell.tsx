/**
 * Aurora Desktop Shell — V23-I5（三栏布局闭环）
 *
 * 布局（V19 §3 桌面信息架构落地）:
 * - Sidebar: 笔记列表 + 新建 + 视图导航（笔记 / 今日）
 * - EditorPane: 选中笔记的标题 + 正文预览 + 元信息; 未选中空状态引导
 * - StatusBar: IPC 模式（tauri / browser-mock）+ 版本 + 今日统计
 *
 * 数据通路（I1 双通路策略承继）:
 * - tauri 模式: invoke(cmd_*) 直连内核
 * - browser-mock 模式: 内存演示数据（当前环境无 Tauri WebView 宿主,
 *   UI 完整性靠此通路验证; Tauri 编译环境补齐后零改动切换）
 *
 * 样式全部引用 design/tokens（单一事实源 — 禁止硬编码色值）。
 * 无障碍: 焦点环 2px focus 色 / 触控目标 ≥44px / 对比度 ≥4.5:1（tokens.a11y）。
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import tokens from '../design/tokens';
import CommandPalette, { type PaletteItem } from './CommandPalette';

export interface InvokeFn {
  (cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

export interface NoteSummary {
  note_id: string;
  title: string;
  updated_at: string;
}

export interface NoteContent {
  note_id: string;
  title: string;
  content: string;
  updated_at: string;
}

export type MainView = 'notes' | 'today';

/** browser-mock 演示数据 — 形状与内核 NoteSummary/NoteContent 对齐 */
const MOCK_NOTES: NoteSummary[] = [
  { note_id: 'n1', title: 'V23 迭代复盘', updated_at: '2026-09-09 21:40' },
  { note_id: 'n2', title: 'NTN HARQ 反馈禁用场景', updated_at: '2026-09-09 20:12' },
  { note_id: 'n3', title: 'Rust 异步锁安全清单', updated_at: '2026-09-08 18:05' },
];

const MOCK_CONTENT: Record<string, NoteContent> = {
  n1: {
    note_id: 'n1',
    title: 'V23 迭代复盘',
    content:
      '# V23 迭代复盘\n\n- I0 安全收口: 审计哈希链落地\n- I2 Mirror 单向导出: data/mirror/ 即活笔记库\n- I4 Agent 现场感知: get_agent_context 四段结构\n- I5 MCP 契约: 四工具 schema 冻结\n\n待办: 传输层接入 / WASM L0 权限 / 时间机器定时器。',
    updated_at: '2026-09-09 21:40',
  },
  n2: {
    note_id: 'n2',
    title: 'NTN HARQ 反馈禁用场景',
    content:
      '# NTN HARQ 反馈禁用场景\n\n跨层调度专利分析要点:\n1. 反馈禁用触发条件（链路质量阈值）\n2. 下行重传调度补偿策略\n3. 与 SR/CSI 复用冲突消解。',
    updated_at: '2026-09-09 20:12',
  },
  n3: {
    note_id: 'n3',
    title: 'Rust 异步锁安全清单',
    content:
      '# Rust 异步锁安全清单\n\n- 持锁跨 await: 编译期不可检, 用扫描脚本\n- tokio::sync::Mutex 优先于 std\n- 死锁四条件与 breaking 策略。',
    updated_at: '2026-09-08 18:05',
  },
};

/** 统一数据门面: tauri invoke 优先, 缺失回落 mock（UI 两模式形状一致） */
function useDataBridge(invoke: InvokeFn | null) {
  return useMemo(
    () => ({
      async listNotes(): Promise<NoteSummary[]> {
        if (invoke) {
          try {
            return (await invoke('cmd_list_notes')) as NoteSummary[];
          } catch {
            /* fallthrough to mock */
          }
        }
        return MOCK_NOTES;
      },
      async getContent(noteId: string): Promise<NoteContent | null> {
        if (invoke) {
          try {
            return (await invoke('cmd_get_note_content', { note_id: noteId })) as NoteContent;
          } catch {
            /* fallthrough to mock */
          }
        }
        return MOCK_CONTENT[noteId] ?? null;
      },
      async todayStats(): Promise<{ active: number; done: number; due_today: number } | null> {
        if (invoke) {
          try {
            return (await invoke('cmd_today_view_stats')) as {
              active: number;
              done: number;
              due_today: number;
            };
          } catch {
            return null;
          }
        }
        return { active: 3, done: 5, due_today: 2 }; // mock
      },
      async createNote(title: string): Promise<void> {
        if (invoke) {
          invoke('cmd_create_note', { title }).catch(() => {});
        }
      },
    }),
    [invoke],
  );
}

const SIDEBAR_W = 248;

function Sidebar(props: {
  notes: NoteSummary[];
  selectedId: string | null;
  view: MainView;
  onView: (v: MainView) => void;
  onSelect: (id: string) => void;
  onCreate: () => void;
}) {
  const { notes, selectedId, view, onView, onSelect, onCreate } = props;
  const itemStyle = (active: boolean): React.CSSProperties => ({
    display: 'block',
    width: '100%',
    textAlign: 'left',
    background: active ? tokens.color.bgElevated : 'transparent',
    color: active ? tokens.color.primaryBright : tokens.color.textPrimary,
    border: 'none',
    borderRadius: tokens.radius.md,
    padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
    fontSize: tokens.typography.body.size,
    lineHeight: tokens.typography.body.lineHeight,
    cursor: 'pointer',
    minHeight: tokens.a11y.minTouchTarget,
    outline: 'none',
  });
  return (
    <aside
      style={{
        width: SIDEBAR_W,
        flexShrink: 0,
        background: tokens.color.bgSurface,
        borderRight: '1px solid rgba(255,255,255,0.08)',
        display: 'flex',
        flexDirection: 'column',
        gap: tokens.spacing.sm,
        padding: tokens.spacing.md,
        overflowY: 'auto',
      }}
    >
      <button
        onClick={onCreate}
        style={{
          background: tokens.color.primary,
          color: '#FFFFFF',
          border: 'none',
          borderRadius: tokens.radius.md,
          padding: `${tokens.spacing.sm + 2}px ${tokens.spacing.md}px`,
          fontSize: tokens.typography.bodyStrong.size,
          fontWeight: tokens.typography.bodyStrong.weight,
          cursor: 'pointer',
          minHeight: tokens.a11y.minTouchTarget,
        }}
      >
        ＋ 新建笔记
      </button>
      <nav style={{ display: 'flex', gap: tokens.spacing.xs }} aria-label="主导航">
        {(['notes', 'today'] as const).map((v) => (
          <button
            key={v}
            onClick={() => onView(v)}
            aria-current={view === v ? 'page' : undefined}
            style={{
              flex: 1,
              background: view === v ? tokens.color.bgElevated : 'transparent',
              color: view === v ? tokens.color.primaryBright : tokens.color.textSecondary,
              border: 'none',
              borderRadius: tokens.radius.md,
              padding: tokens.spacing.sm,
              fontSize: tokens.typography.caption.size + 1,
              cursor: 'pointer',
              minHeight: 36,
            }}
          >
            {v === 'notes' ? '全部笔记' : '今日视图'}
          </button>
        ))}
      </nav>
      <div
        style={{
          fontSize: tokens.typography.caption.size,
          color: tokens.color.textSecondary,
          padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
        }}
      >
        笔记 · {notes.length}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
        {notes.map((n) => (
          <button
            key={n.note_id}
            onClick={() => onSelect(n.note_id)}
            style={itemStyle(selectedId === n.note_id)}
            onFocus={(e) => {
              e.currentTarget.style.outline = `${tokens.a11y.focusRingWidth}px solid ${tokens.color.focus}`;
            }}
            onBlur={(e) => {
              e.currentTarget.style.outline = 'none';
            }}
          >
            <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {n.title || '未命名'}
            </span>
            <span style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
              {n.updated_at}
            </span>
          </button>
        ))}
        {notes.length === 0 && (
          <p
            style={{
              color: tokens.color.textSecondary,
              fontSize: tokens.typography.caption.size,
              padding: tokens.spacing.md,
            }}
          >
            还没有笔记 — 点上方「新建笔记」开始。
          </p>
        )}
      </div>
    </aside>
  );
}

function EditorPane(props: { note: NoteContent | null; loading: boolean }) {
  const { note, loading } = props;
  if (loading) {
    return (
      <main style={{ flex: 1, padding: tokens.spacing.lg, color: tokens.color.textSecondary }}>
        加载中…
      </main>
    );
  }
  if (!note) {
    return (
      <main
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: tokens.spacing.md,
          color: tokens.color.textSecondary,
        }}
      >
        <span style={{ fontSize: 40 }}>🌙</span>
        <p style={{ margin: 0, fontSize: tokens.typography.heading.size, color: tokens.color.textPrimary }}>
          选择左侧笔记开始
        </p>
        <p style={{ margin: 0, fontSize: tokens.typography.body.size }}>
          或按 Mod+K 唤起命令面板 · 搜索 / 新建 / 今日视图
        </p>
      </main>
    );
  }
  return (
    <main
      style={{
        flex: 1,
        padding: `${tokens.spacing.lg}px ${tokens.spacing.xl}px`,
        overflowY: 'auto',
        minWidth: 0,
      }}
    >
      <h1
        style={{
          margin: `0 0 ${tokens.spacing.sm}px`,
          fontSize: tokens.typography.title.size,
          lineHeight: tokens.typography.title.lineHeight,
          color: tokens.color.textPrimary,
        }}
      >
        {note.title || '未命名'}
      </h1>
      <p
        style={{
          margin: `0 0 ${tokens.spacing.lg}px`,
          fontSize: tokens.typography.caption.size,
          color: tokens.color.textSecondary,
        }}
      >
        更新于 {note.updated_at} · 只读预览（编辑器 Shell 按 I2 块级编辑挂载）
      </p>
      <pre
        style={{
          margin: 0,
          whiteSpace: 'pre-wrap',
          fontFamily: tokens.typography.mono,
          fontSize: tokens.typography.body.size,
          lineHeight: tokens.typography.body.lineHeight,
          color: tokens.color.textPrimary,
        }}
      >
        {note.content}
      </pre>
    </main>
  );
}

function StatusBar(props: {
  mode: string;
  stats: { active: number; done: number; due_today: number } | null;
}) {
  const { mode, stats } = props;
  return (
    <footer
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: tokens.spacing.md,
        padding: `${tokens.spacing.xs}px ${tokens.spacing.md}px`,
        background: tokens.color.bgSurface,
        borderTop: '1px solid rgba(255,255,255,0.08)',
        fontSize: tokens.typography.caption.size,
        color: tokens.color.textSecondary,
      }}
    >
      <span>
        IPC: {mode === 'tauri' ? 'Tauri 内核' : 'browser-mock（演示数据）'}
      </span>
      {stats && (
        <span>
          今日: 进行中 {stats.active} · 已完成 {stats.done} · 到期 {stats.due_today}
        </span>
      )}
      <span style={{ marginLeft: 'auto' }}>Aurora Note v0.23</span>
    </footer>
  );
}

export default function DesktopShell() {
  const [invoke, setInvoke] = useState<InvokeFn | null>(null);
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [content, setContent] = useState<NoteContent | null>(null);
  const [loading, setLoading] = useState(false);
  const [view, setView] = useState<MainView>('notes');
  const [stats, setStats] = useState<{ active: number; done: number; due_today: number } | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [results, setResults] = useState<PaletteItem[]>([]);

  const data = useDataBridge(invoke);

  useEffect(() => {
    // Tauri IPC 探测（与 bootstrap.ts 同语义）; 当前环境回落 browser-mock
    import('@tauri-apps/api/core')
      .then((m) => setInvoke(m.invoke as InvokeFn))
      .catch(() => setInvoke(null));
  }, []);

  useEffect(() => {
    data.listNotes().then(setNotes);
    data.todayStats().then(setStats);
  }, [data]);

  useEffect(() => {
    if (!selectedId) {
      setContent(null);
      return;
    }
    setLoading(true);
    data.getContent(selectedId).then((c) => {
      setContent(c);
      setLoading(false);
    });
  }, [selectedId, data]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
      if (e.key === 'Escape') setPaletteOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  /** 原始行形状（CommandPalette 契约: note_id/title/snippet/score） */
  type RawHit = { note_id: string; title: string; snippet: string; score: number };
  const searchNotes = useCallback(
    async (q: string): Promise<RawHit[]> => {
      if (invoke) {
        try {
          return (await invoke('cmd_search_notes', { query: q })) as RawHit[];
        } catch {
          /* fallthrough to mock */
        }
      }
      // mock: 本地标题过滤
      return notes
        .filter((n) => n.title.includes(q))
        .map((n, i) => ({
          note_id: n.note_id,
          title: n.title,
          snippet: '（演示数据）',
          score: 1 - i * 0.1,
        }));
    },
    [invoke, notes],
  );

  const runSearch = useCallback(
    async (q: string) => {
      const hits = await searchNotes(q);
      setResults(
        hits.map((r) => ({
          kind: 'note' as const,
          id: r.note_id,
          title: r.title,
          snippet: r.snippet,
          run: () => setSelectedId(r.note_id),
        })),
      );
    },
    [searchNotes],
  );

  const builtins: PaletteItem[] = useMemo(
    () => [
      {
        kind: 'command',
        id: 'new-note',
        title: '新建笔记',
        run: () => data.createNote('未命名'),
      },
      {
        kind: 'command',
        id: 'today',
        title: `今日视图${stats ? `（进行中 ${stats.active} · 今日到期 ${stats.due_today}）` : ''}`,
        run: () => setView('today'),
      },
    ],
    [data, stats],
  );

  const mode = invoke ? 'tauri' : 'browser-mock';

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: tokens.color.bgBase,
        color: tokens.color.textPrimary,
        fontFamily: tokens.typography.family,
      }}
    >
      <header
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: tokens.spacing.md,
          padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
          background: tokens.color.bgSurface,
          borderBottom: '1px solid rgba(255,255,255,0.08)',
        }}
      >
        <strong style={{ fontSize: tokens.typography.heading.size }}>Aurora Note</strong>
        <input
          placeholder="搜索 — Mod+K（口语化可用）"
          onFocus={() => setPaletteOpen(true)}
          readOnly
          aria-label="全局搜索"
          style={{
            flex: 1,
            background: tokens.color.bgElevated,
            border: '1px solid rgba(255,255,255,0.10)',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.sm + 2}px ${tokens.spacing.md}px`,
            color: tokens.color.textPrimary,
            fontSize: tokens.typography.body.size,
            outline: 'none',
          }}
        />
        {view === 'today' && stats && (
          <span style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
            今日视图 · 进行中 {stats.active} / 已完成 {stats.done}
          </span>
        )}
      </header>
      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        <Sidebar
          notes={notes}
          selectedId={selectedId}
          view={view}
          onView={setView}
          onSelect={(id) => {
            setSelectedId(id);
            setView('notes');
          }}
          onCreate={() => data.createNote('未命名')}
        />
        {view === 'today' ? (
          <main style={{ flex: 1, padding: tokens.spacing.lg }}>
            <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>
              今日视图
            </h1>
            {stats ? (
              <div style={{ display: 'flex', gap: tokens.spacing.md }}>
                {(
                  [
                    ['进行中', stats.active, tokens.color.warning],
                    ['已完成', stats.done, tokens.color.success],
                    ['今日到期', stats.due_today, tokens.color.danger],
                  ] as const
                ).map(([label, n, c]) => (
                  <div
                    key={label}
                    style={{
                      background: tokens.color.bgSurface,
                      border: '1px solid rgba(255,255,255,0.08)',
                      borderRadius: tokens.radius.lg,
                      padding: tokens.spacing.md,
                      minWidth: 120,
                    }}
                  >
                    <div style={{ fontSize: 28, fontWeight: 700, color: c }}>{n}</div>
                    <div style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
                      {label}（图标+文字双载体）
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p style={{ color: tokens.color.textSecondary }}>统计加载中…</p>
            )}
          </main>
        ) : (
          <EditorPane note={content} loading={loading} />
        )}
      </div>
      <StatusBar mode={mode} stats={stats} />
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        searchNotes={searchNotes}
        onQuery={runSearch}
        openNote={(id) => setSelectedId(id)}
        items={[...builtins, ...results]}
      />
    </div>
  );
}
