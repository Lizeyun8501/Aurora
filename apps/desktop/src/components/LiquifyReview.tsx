/**
 * DK-10 切片 4/7 AI 提案审查视图（两段式提交的用户侧 UI）——core 面见 8ea9a18 前身 bc22dea。
 *
 * 铁律可视化：AI 不得静默写入——提案（aiLiquify 产物）在此由用户**逐 op 勾选**，
 * 勾选后才 `cmd_ai_commit_liquify`（走 write_path 标准写入）；拒绝走 reject（终态）。
 * 提交结果按 op 诚实展示（部分失败不掩盖——OpResult 清单）。
 *
 * browser-mock 模式（invoke = null）显示提示态（命令仅 tauri 模式可用）。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

interface OpResult {
  op_index: number;
  ok: boolean;
  note_id: string | null;
  error: string | null;
}

interface LiquifyProposal {
  id: string;
  created_at: number;
  source: string;
  status: 'draft' | 'committed' | 'rejected';
  ops: Array<
    | { type: 'create_note'; title: string; content: string }
    | { type: 'update_note'; note_id: string; new_content: string; expected_title: string }
  >;
  results?: OpResult[];
}

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

const btn = {
  background: tokens.color.bgElevated,
  color: tokens.color.textPrimary,
  border: '1px solid rgba(255,255,255,0.12)',
  borderRadius: tokens.radius.md,
  padding: `${tokens.spacing.xs + 2}px ${tokens.spacing.sm + 2}px`,
  fontSize: tokens.typography.caption.size,
  cursor: 'pointer',
  minHeight: 32,
} as const;

const primaryBtn = {
  ...btn,
  background: tokens.color.primaryBright,
  color: '#10141a',
  border: 'none',
  fontWeight: 600,
} as const;

const badge = (status: LiquifyProposal['status']) =>
  ({
    draft: { text: '待审', color: tokens.color.warning },
    committed: { text: '已提交', color: tokens.color.success },
    rejected: { text: '已拒绝', color: tokens.color.textSecondary },
  })[status];

function opLabel(op: LiquifyProposal['ops'][number]): string {
  if (op.type === 'create_note') {
    return `创建笔记《${op.title}》（正文 ${op.content.length} 字）`;
  }
  return `更新笔记《${op.expected_title}》正文（新内容 ${op.new_content.length} 字）`;
}

export default function LiquifyReview({ invoke }: { invoke: InvokeFn | null }): React.ReactElement {
  const [items, setItems] = useState<LiquifyProposal[] | null>(null);
  const [selected, setSelected] = useState<Record<string, Set<number>>>({});
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    if (!invoke) return;
    invoke('cmd_ai_list_liquify_proposals')
      .then((rows) => {
        const list = (rows as string[]).map((r) => JSON.parse(r) as LiquifyProposal);
        list.sort((a, b) => b.created_at - a.created_at);
        setItems(list);
        setSelected({});
      })
      .catch((e) => setError(String(e)));
  }, [invoke]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const toggleOp = (pid: string, idx: number) => {
    setSelected((prev) => {
      const next = { ...prev };
      const cur = new Set(next[pid] ?? []);
      if (cur.has(idx)) cur.delete(idx);
      else cur.add(idx);
      next[pid] = cur;
      return next;
    });
  };

  const commit = async (p: LiquifyProposal) => {
    if (!invoke) return;
    const sel = Array.from(selected[p.id] ?? []).sort((a, b) => a - b);
    if (sel.length === 0) return;
    setBusyId(p.id);
    setError(null);
    try {
      await invoke('cmd_ai_commit_liquify', { proposalId: p.id, selected: sel });
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const reject = async (p: LiquifyProposal) => {
    if (!invoke) return;
    setBusyId(p.id);
    setError(null);
    try {
      await invoke('cmd_ai_reject_liquify_proposal', { proposalId: p.id });
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  if (!invoke) {
    return (
      <main style={{ flex: 1, padding: tokens.spacing.lg }}>
        <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>
          AI 提案审查
        </h1>
        <p style={{ color: tokens.color.textSecondary }}>
          提案审查需 tauri 模式（core 命令 aiLiquify/aiCommit 仅桌面可用）。
        </p>
      </main>
    );
  }

  return (
    <main style={{ flex: 1, padding: tokens.spacing.lg, overflowY: 'auto' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: tokens.spacing.md }}>
        <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>
          AI 提案审查
        </h1>
        <button style={{ ...btn, marginBottom: tokens.spacing.md }} onClick={refresh}>
          刷新
        </button>
      </div>
      <p style={{ margin: `0 0 ${tokens.spacing.md}px`, color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
        铁律：AI 不得静默写入——勾选你认可的提案操作后提交，未勾选的操作不会落库。
      </p>
      {error && <p style={{ color: tokens.color.danger }}>{error}</p>}
      {!items ? (
        <p style={{ color: tokens.color.textSecondary }}>加载中…</p>
      ) : items.length === 0 ? (
        <p style={{ color: tokens.color.textSecondary }}>暂无提案（AI 通过 aiLiquify 产出提案后出现在这里）。</p>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.md }}>
          {items.map((p) => {
            const b = badge(p.status);
            const sel = selected[p.id] ?? new Set<number>();
            const hasResults = (p.results?.length ?? 0) > 0;
            return (
              <div
                key={p.id}
                style={{
                  background: tokens.color.bgElevated,
                  border: '1px solid rgba(255,255,255,0.08)',
                  borderRadius: tokens.radius.lg,
                  padding: tokens.spacing.md,
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: tokens.spacing.sm, marginBottom: tokens.spacing.xs }}>
                  <span
                    style={{
                      fontSize: tokens.typography.caption.size,
                      color: b.color,
                      border: `1px solid ${b.color}`,
                      borderRadius: tokens.radius.sm,
                      padding: '0 6px',
                    }}
                  >
                    {b.text}
                  </span>
                  <span style={{ fontWeight: 600 }}>{p.source}</span>
                  <span style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
                    {new Date(p.created_at).toLocaleTimeString()} · {p.id.slice(0, 12)}
                  </span>
                </div>
                <ul style={{ margin: `0 0 ${tokens.spacing.sm}px`, paddingLeft: 20 }}>
                  {p.ops.map((op, i) => (
                    <li key={i} style={{ marginBottom: 4 }}>
                      {p.status === 'draft' ? (
                        <label style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                          <input
                            type="checkbox"
                            checked={sel.has(i)}
                            onChange={() => toggleOp(p.id, i)}
                          />
                          <span>{opLabel(op)}</span>
                        </label>
                      ) : (
                        <span>{opLabel(op)}</span>
                      )}
                    </li>
                  ))}
                </ul>
                {hasResults && (
                  <div style={{ marginBottom: tokens.spacing.sm }}>
                    {p.results!.map((r) => (
                      <div
                        key={r.op_index}
                        style={{
                          fontSize: tokens.typography.caption.size,
                          color: r.ok ? tokens.color.success : tokens.color.danger,
                        }}
                      >
                        {r.ok ? '✓' : '✗'} op#{r.op_index} {r.ok ? `落库 ${r.note_id}` : `失败：${r.error}`}
                      </div>
                    ))}
                  </div>
                )}
                {p.status === 'draft' && (
                  <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
                    <button
                      style={primaryBtn}
                      disabled={busyId === p.id || sel.size === 0}
                      onClick={() => commit(p)}
                    >
                      提交所选（{sel.size}）
                    </button>
                    <button style={btn} disabled={busyId === p.id} onClick={() => reject(p)}>
                      拒绝
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </main>
  );
}
