/**
 * Aurora Desktop React Shell — V23-I1（双端统一组件策略起点）
 *
 * 结构：
 * - AppShell: 顶栏（搜索入口/今日视图统计） + 内容区（编辑器占位）
 * - CommandPalette: Mod+K 全局命令面板（cmd_search_notes / 命令注册表）
 * - Design Tokens: src/design/tokens.ts 单一事实源
 *
 * 后续迭代挂载: EditorShell（I2 块级）/ Timeline 时间机器（I3）/ 画布（I4）
 */
import { useCallback, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import tokens from './design/tokens';
import CommandPalette, { type PaletteItem } from './components/CommandPalette';

interface InvokeFn {
  (cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

/** Tauri IPC 探测 — adapters/ipc.ts 的精简路径（Shell 用） */
async function getInvoke(): Promise<InvokeFn | null> {
  try {
    const mod = await import('@tauri-apps/api/core');
    return mod.invoke as InvokeFn;
  } catch {
    return null; // browser-mock 模式（bootstrap.ts 同语义）
  }
}

function AppShell() {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [stats, setStats] = useState<{ active: number; done: number; due_today: number } | null>(null);
  const [invoke, setInvoke] = useState<InvokeFn | null>(null);
  const [results, setResults] = useState<PaletteItem[]>([]);

  useEffect(() => {
    getInvoke().then((fn) => {
      setInvoke(fn);
      if (fn) {
        fn('cmd_today_view_stats')
          .then((s) => setStats(s as typeof stats))
          .catch(() => setStats(null));
      }
    });
  }, []);

  // Mod+K 全局唤起（V22.1 §6.6 快捷键表）
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

  const searchNotes = useCallback(
    async (q: string) => {
      if (!invoke) return [];
      try {
        return (await invoke('cmd_search_notes', { query: q })) as Array<{
          note_id: string;
          title: string;
          snippet: string;
          score: number;
        }>;
      } catch {
        return [];
      }
    },
    [invoke],
  );

  const runSearch = useCallback(
    async (q: string) => {
      const items = await searchNotes(q);
      setResults(
        items.map((r) => ({
          kind: 'note' as const,
          id: r.note_id,
          title: r.title,
          snippet: r.snippet,
          run: () => {
            /* I2: EditorShell 打开笔记 */
          },
        })),
      );
    },
    [searchNotes],
  );

  const builtins: PaletteItem[] = useMemo(
    () => [
      { kind: 'command', id: 'new-note', title: '新建笔记', run: () => { invoke?.('cmd_create_note', { title: '未命名' }).catch(() => {}); } },
      { kind: 'command', id: 'today', title: `今日视图${stats ? `（进行中 ${stats.active} · 今日到期 ${stats.due_today}）` : ''}`, run: () => { /* I2: TodayView 路由 */ } },
      { kind: 'command', id: 'review', title: '开始复习（FSRS）', run: () => { invoke?.('cmd_due_review_cards').catch(() => {}); } },
    ],
    [invoke, stats],
  );

  const items = [...builtins, ...results];

  return (
    <div
      style={{
        minHeight: '100vh',
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
        <strong style={{ fontSize: tokens.typography.title.size }}>Aurora Note</strong>
        <input
          placeholder="搜索 — Mod+K（口语化可用）"
          onFocus={() => setPaletteOpen(true)}
          readOnly
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
        {stats && (
          <span style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
            进行中 {stats.active} · 今日到期 {stats.due_today}
          </span>
        )}
      </header>
      <main style={{ padding: tokens.spacing.lg }}>
        <p style={{ color: tokens.color.textSecondary }}>
          I1 桌面 Alpha：命令面板（Mod+K）+ 今日视图统计已接内核。
          编辑器/时间机器/画布按 I2-I4 迭代挂载。
        </p>
      </main>
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        searchNotes={async (q) => (await searchNotes(q)) ?? []}
        onQuery={runSearch}
        openNote={() => {}}
        commands={builtins.map((b) => ({ id: b.id, title: b.title, run: b.run }))}
        items={items}
      />
    </div>
  );
}

const el = document.getElementById('root');
if (el) {
  createRoot(el).render(<AppShell />);
}
