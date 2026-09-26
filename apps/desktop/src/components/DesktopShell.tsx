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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import tokens from '../design/tokens';
import type { AuroraEditorHandle } from '@aurora/ui-components';
import CommandPalette, { type PaletteItem } from './CommandPalette';
import ImportWizard from './ImportWizard';

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

/** `cmd_read_attachment` 返回 DTO（与 src-tauri attachment_commands 对齐） */
export interface ReadAttachment {
  attachment_id: string;
  note_id: string;
  file_name: string;
  mime: string;
  size: number;
  data_base64: string;
}

/** 正文中 `attachment://` 引用（DK-09 引用形态定调：`attachment://{id}`） */
export interface AttachmentRef {
  /** id（scheme 后本体） */
  id: string;
  /** markdown 链接 alt/文本（`![alt](...)` / `[alt](...)` 的 alt 段） */
  label: string;
  /** true = 图片语法 `![...]`（预览内联渲染），false = 普通链接 */
  isImage: boolean;
}

const ATTACHMENT_LINK_RE = /(!?)\[([^\]\n]*)\]\(attachment:\/\/([^)\s]+)\)/g;

/**
 * 从 markdown 正文抽取 attachment:// 引用（DK-09 渲染接线）。
 * - 只识别定调形态 `[...](attachment://{id})`（含图片 `!` 前缀）；
 * - 同 id 多次引用去重（首处 label/形态生效）；
 * - id 过白名单校验（与 core validate_attachment_id 同规则）——非法形态
 *   直接忽略，不发 IPC（防注入面收敛在前端入口）。
 */
export function extractAttachmentRefs(content: string): AttachmentRef[] {
  const seen = new Map<string, AttachmentRef>();
  for (const m of content.matchAll(ATTACHMENT_LINK_RE)) {
    const [, bang, label, id] = m;
    if (seen.has(id)) continue;
    if (!/^[A-Za-z0-9_-]{1,128}$/.test(id)) continue;
    seen.set(id, { id, label: label || id, isImage: bang === '!' });
  }
  return [...seen.values()];
}

/** 人类可读大小（附件卡展示） */
function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
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
            // DK-05 S2: 修正命令名 — 内核 handler 是 cmd_get_note（原
            // cmd_get_note_content 不存在，tauri 模式下会 404 回落 mock）
            return (await invoke('cmd_get_note', { note_id: noteId })) as NoteContent;
          } catch {
            /* fallthrough to mock */
          }
        }
        return MOCK_CONTENT[noteId] ?? null;
      },
      /** DK-05 S2 落库：编辑产物（markdown-ish 文本）经既有 cmd_update_note（禁绕过） */
      async saveContent(noteId: string, content: string): Promise<void> {
        if (invoke) {
          await invoke('cmd_update_note', { note_id: noteId, content });
          return;
        }
        // browser-mock: 内存更新（往返一致验证通路）+ 测试钩子
        if (MOCK_CONTENT[noteId]) MOCK_CONTENT[noteId].content = content;
        (window as any).__lastSaved = { noteId, content };
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
  /** DK-09：打开迁移向导（ImportWizard 挂载入口）。 */
  onImport: () => void;
}) {
  const { notes, selectedId, view, onView, onSelect, onCreate, onImport } = props;
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
      {/* DK-09：迁移向导入口（ImportWizard，授权切片） */}
      <button
        onClick={onImport}
        style={{
          background: 'transparent',
          color: tokens.color.textSecondary,
          border: 'none',
          borderRadius: tokens.radius.md,
          padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
          fontSize: tokens.typography.body.size,
          cursor: 'pointer',
          minHeight: tokens.a11y.minTouchTarget,
          textAlign: 'left',
        }}
      >
        ⤓ 导入笔记
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

/** 单个附件项：加载 `attachment://{id}` → 图片内联 / 文件卡 */
function AttachmentItem(props: { invoke: InvokeFn | null; id: string; label: string; isImage: boolean }) {
  const { invoke, id, label, isImage } = props;
  const [data, setData] = useState<ReadAttachment | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    if (!invoke) {
      // browser-mock 模式（无 Tauri 宿主）：无内核可读，保持提示态
      setError('mock 模式无附件内核');
      return () => {
        cancelled = true;
      };
    }
    invoke('cmd_read_attachment', { attachment_id: id })
      .then((r) => {
        if (!cancelled) setData(r as ReadAttachment);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [invoke, id]);

  if (error) {
    return (
      <div
        style={{
          padding: tokens.spacing.sm,
          borderRadius: tokens.radius?.md ?? 6,
          border: `1px solid ${tokens.color.bgElevated}`,
          color: tokens.color.textSecondary,
          fontSize: tokens.typography.caption.size,
        }}
      >
        附件不可读: {label}（{error}）
      </div>
    );
  }
  if (!data) {
    return (
      <div style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
        加载附件… {label}
      </div>
    );
  }
  const src = `data:${data.mime};base64,${data.data_base64}`;
  if (isImage && data.mime.startsWith('image/')) {
    return (
      <figure style={{ margin: 0, display: 'flex', flexDirection: 'column', gap: tokens.spacing.xs }}>
        <img
          src={src}
          alt={label}
          style={{ maxWidth: '100%', borderRadius: tokens.radius?.md ?? 6, border: `1px solid ${tokens.color.bgElevated}` }}
        />
        <figcaption style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
          {data.file_name} · {formatBytes(data.size)}
        </figcaption>
      </figure>
    );
  }
  return (
    <a
      href={src}
      download={data.file_name}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: tokens.spacing.xs,
        padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
        borderRadius: tokens.radius?.md ?? 6,
        border: `1px solid ${tokens.color.bgElevated}`,
        color: tokens.color.focus ?? tokens.color.textPrimary,
        textDecoration: 'none',
        fontSize: tokens.typography.caption.size,
      }}
    >
      📎 {data.file_name} · {formatBytes(data.size)}
    </a>
  );
}

/** 附件区：正文 attachment:// 引用的解析展示（无引用时不渲染） */
function AttachmentStrip(props: { invoke: InvokeFn | null; content: string }) {
  const { invoke, content } = props;
  const refs = useMemo(() => extractAttachmentRefs(content), [content]);
  if (refs.length === 0) return null;
  return (
    <div style={{ marginTop: tokens.spacing.md, display: 'flex', flexDirection: 'column', gap: tokens.spacing.sm }}>
      {refs.map((r) => (
        <AttachmentItem key={r.id} invoke={invoke} id={r.id} label={r.label} isImage={r.isImage} />
      ))}
    </div>
  );
}

function EditorPane(props: {
  note: NoteContent | null;
  loading: boolean;
  invoke: InvokeFn | null;
  persistNote: (noteId: string, content: string) => Promise<void>;
}) {
  const { note, loading, invoke, persistNote } = props;
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
        更新于 {note.updated_at} · DK-05 S2 · 编辑态（工具条块操作 / 防抖落库）
      </p>
      {/* DK-05 S2: 可编辑态挂载（S1 只读 → 编辑态 + 块操作工具条 +
          markdown-ish 序列化落库 cmd_update_note + 防抖/flush） */}
      <EditAuroraEditor
        noteId={note.note_id}
        content={note.content}
        onPersist={(text) => void persistNote(note.note_id, text)}
      />
      {/* DK-09 渲染接线：attachment:// 引用解析（图片内联 / 文件卡） */}
      <AttachmentStrip invoke={invoke} content={note.content} />
    </main>
  );
}

/**
 * DK-05 S2 可编辑编辑器 — 共享层实体挂载（loro-prosemirror 管线）+ 块操作。
 *
 * - 内容链路：content（markdown-ish）→ mdToNodes 初始 doc；编辑产物
 *   docToMd → onPersist → cmd_update_note（禁绕过既有 command）；
 * - 防抖/flush：createAuroraEditor 内建 onSave debounce 1s（scheduleSave），
 *   unmount/切笔记 cleanup 先 flushSave()（同步）再 destroy（退出前 flush）；
 * - 块操作：共享层 EditorToolbar（heading/列表/task/code_block/undo/redo，
 *   LoroUndoPlugin 撤销栈）；onUpdate 刷新激活态（tick）；
 * - a11y（R-04 A 项·正文区）：role=textbox + aria-label + tabIndex=0 +
 *   focus 焦点环；工具条 role=toolbar 原生 button（Tab 可达 + Enter 激活）。
 */
function EditAuroraEditor(props: {
  noteId: string;
  content: string;
  onPersist: (text: string) => void;
}) {
  const { noteId, content, onPersist } = props;
  const hostRef = useRef<HTMLDivElement | null>(null);
  const [focused, setFocused] = useState(false);
  const [view, setView] = useState<import('prosemirror-view').EditorView | null>(null);
  const [tick, setTick] = useState(0);
  const [Toolbar, setToolbar] = useState<React.ComponentType<{ view: import('prosemirror-view').EditorView | null; tick: number }> | null>(null);
  const persistRef = useRef(onPersist);
  persistRef.current = onPersist;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let handle: AuroraEditorHandle | null = null;
    let cancelled = false;
    (async () => {
      const { createAuroraEditor, docToMd, mdToNodes, EditorToolbar: ET } = await import('@aurora/ui-components');
      const { LoroDoc } = await import('loro-crdt');
      setToolbar(() => ET);
      if (cancelled || !hostRef.current) return;
      handle = createAuroraEditor(host, {
        loroDoc: new LoroDoc(),
        onSave: () => {
          // debounce 1s 到期（或 flushSave）——编辑产物序列化落库
          if (handle) persistRef.current(docToMd(handle.view.state.doc));
        },
        onUpdate: () => setTick((t) => t + 1), // 工具条激活态刷新
      });
      const v = handle.view;
      // markdown-ish → 初始 doc（LoroSync 初始同步前写入，双方收敛一致）
      const nodes = mdToNodes(v.state.schema, content);
      if (nodes.length) {
        v.dispatch(v.state.tr.replaceWith(0, v.state.doc.content.size, nodes));
      }
      if (!cancelled) setView(v);
    })().catch((e) => console.error('EditAuroraEditor init failed', e));
    return () => {
      cancelled = true;
      setView(null);
      // 退出前 flush：同步序列化落库（切笔记/unmount 语义），再销毁
      try {
        handle?.flushSave();
      } catch { /* 初始同步未完成时无内容可flush */ }
      handle?.destroy();
    };
  }, [noteId, content]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.xs }}>
      {/* 桌面工具条样式 — className 与共享层 EditorToolbar 对齐（mobile.css 桌面等价，
          token 单一事实源） */}
      <style>{`
        .editor-toolbar { display: flex; align-items: center; gap: 2px;
          padding: 4px 0; overflow-x: auto; scrollbar-width: none; }
        .editor-toolbar::-webkit-scrollbar { display: none; }
        .tb-btn { min-width: 32px; height: 32px; display: flex; align-items: center;
          justify-content: center; border: none; border-radius: 6px;
          background: transparent; color: ${tokens.color.textSecondary};
          font-size: 14px; cursor: pointer; }
        .tb-btn:hover { background: ${tokens.color.bgElevated}; }
        .tb-btn.active { background: ${tokens.color.focus}22;
          color: ${tokens.color.focus}; font-weight: 600; }
        .tb-sep { flex: 0 0 1px; height: 18px; margin: 0 6px;
          background: ${tokens.color.textDisabled}; }
        .ProseMirror { outline: none; min-height: 120px; }
      `}</style>
      {/* 块操作工具条（共享层实体）：heading 升降级/列表/task/code_block/undo/redo；
          原生 button — Tab 可达 + Enter/Space 激活（R-04 键盘全操作路径） */}
      {view && Toolbar && <Toolbar view={view} tick={tick} />}
      <div
        ref={hostRef}
        role="textbox"
        aria-multiline="true"
        aria-label="笔记正文"
        tabIndex={0}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        style={{
          outline: 'none',
          ...(focused
            ? {
                outline: `${tokens.a11y.focusRingWidth}px solid ${tokens.color.focus}`,
                outlineOffset: 2,
              }
            : {}),
          cursor: 'text',
        }}
      />
    </div>
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
  /** DK-09：迁移向导开关（内容区条件渲染，不动 MainView 类型面）。 */
  const [wizardOpen, setWizardOpen] = useState(false);

  const data = useDataBridge(invoke);

  useEffect(() => {
    // Tauri IPC 探测（与 bootstrap.ts 同语义）; 当前环境回落 browser-mock。
    // 注意：@tauri-apps/api 包在纯浏览器也可 import 成功，但 invoke 底层依赖
    // window.__TAURI_INTERNALS__（Tauri v2 注入）——必须同时检查宿主标志，
    // 否则 invoke 调用期抛错整树白屏（S1 验证中发现的探测缺陷）。
    Promise.all([
      import('@tauri-apps/api/core'),
      Promise.resolve(typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window),
    ])
      .then(([m, hasHost]) => setInvoke(hasHost ? (m.invoke as InvokeFn) : null))
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

  // DK-08 §7.3：仅 Wi-Fi 同步开关（tauri 模式可用；mock 模式隐藏）
  const [wifiOnly, setWifiOnly] = useState<boolean | null>(null);
  useEffect(() => {
    if (!invoke) return;
    invoke('cmd_get_wifi_only')
      .then((r) => setWifiOnly(r as boolean))
      .catch(() => setWifiOnly(null));
  }, [invoke]);
  const toggleWifiOnly = useCallback(() => {
    if (!invoke || wifiOnly === null) return;
    const next = !wifiOnly;
    invoke('cmd_set_wifi_only', { on: next })
      .then(() => setWifiOnly(next))
      .catch(() => {});
  }, [invoke, wifiOnly]);

  const builtins: PaletteItem[] = useMemo(
    () => [
      {
        kind: 'command',
        id: 'new-note',
        title: '新建笔记',
        run: () => data.createNote('未命名'),
      },
      ...(wifiOnly !== null
        ? [
            {
              kind: 'command' as const,
              id: 'wifi-only',
              title: `仅 Wi-Fi 同步${wifiOnly ? '（已开启 — 点击关闭）' : '（已关闭 — 点击开启）'}`,
              run: toggleWifiOnly,
            },
          ]
        : []),
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
          onImport={() => setWizardOpen(true)}
        />
        {wizardOpen ? (
          /* DK-09 迁移向导（Bravo 授权切片：仅挂载调用，组件自包含） */
          <main style={{ flex: 1, padding: tokens.spacing.lg, overflowY: 'auto' }}>
            <ImportWizard invoke={invoke} onClose={() => setWizardOpen(false)} />
          </main>
        ) : view === 'today' ? (
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
          <EditorPane note={content} loading={loading} invoke={invoke} persistNote={data.saveContent} />
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
