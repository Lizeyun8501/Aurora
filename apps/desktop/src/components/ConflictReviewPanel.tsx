/**
 * ConflictReviewPanel — DK-48 组件 3：冲突逐条决策
 *
 * 数据源（只读消费，Rust 零变更）：
 * - cmd_conflict_list → ConflictDto（kind/id/doc_id/summary/created_at 全展示）
 * - cmd_conflict_resolve(artifact_id) → 副本留存（真冲突副本闭环）
 * - cmd_conflict_resolve_semantic(conflict_id, resolution) → 最新写入优先
 *   （LastWriteWins，照实传参）
 *
 * 决策按钮按 kind 分派（command 数据面本身形状）：artifact 条目
 * 「最新写入优先」置灰（resolve_semantic 仅收语义冲突 id——传 artifact id
 * 必 Err）；semantic 条目「副本留存」置灰（mark_resolved 仅收 artifact）。
 * 工单「每条两个决策位」按此兑现为两决策位 + kind 分派可用性。
 *
 * 决策后局部更新（本地 list 移除该条，不整页 reload）；
 * 批量区「全部按最新写入优先」逐条调用（artifact 不适用 LWW，跳过并计数）。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

/** 与 sync_commands.rs ConflictDto 对齐。 */
interface ConflictDto {
  kind: 'artifact' | 'semantic';
  id: string;
  doc_id: string;
  summary: string;
  created_at: string;
}

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface Props {
  invoke: InvokeFn;
  onClose: () => void;
  /** 有决策落地后通知壳刷新（badge 红点/状态计数） */
  onChanged?: () => void;
}

/** id 短码（展示用，完整 id 保留在决策调用中）。 */
function shortId(id: string): string {
  return id.length > 8 ? `${id.slice(0, 8)}…` : id;
}

export default function ConflictReviewPanel({ invoke, onClose, onChanged }: Props) {
  const [items, setItems] = useState<ConflictDto[] | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [rowErr, setRowErr] = useState<Record<string, string>>({});
  const [busyId, setBusyId] = useState<string | null>(null);
  const [batchBusy, setBatchBusy] = useState(false);
  const [batchNote, setBatchNote] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke('cmd_conflict_list')
      .then((r) => {
        setItems(r as ConflictDto[]);
        setLoadErr(null);
      })
      .catch((e) => {
        setItems(null);
        setLoadErr(String(e));
      });
  }, [invoke]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  /** 单条决策：成功即本地移除（局部更新）+ 通知壳。 */
  const resolveOne = useCallback(
    (item: ConflictDto, mode: 'keep' | 'lww') => {
      setBusyId(item.id);
      setRowErr((m) => {
        const next = { ...m };
        delete next[item.id];
        return next;
      });
      const call =
        mode === 'keep'
          ? invoke('cmd_conflict_resolve', { artifact_id: item.id })
          : invoke('cmd_conflict_resolve_semantic', {
              conflict_id: item.id,
              resolution: 'LastWriteWins',
            });
      call
        .then(() => {
          setItems((list) => (list ? list.filter((x) => x.id !== item.id) : list));
          setBusyId(null);
          onChanged?.();
        })
        .catch((e) => {
          setRowErr((m) => ({ ...m, [item.id]: String(e) }));
          setBusyId(null);
        });
    },
    [invoke, onChanged],
  );

  /** 批量：全部语义冲突按最新写入优先（逐条调用；artifact 跳过并计数）。 */
  const resolveAllLww = useCallback(() => {
    if (!items) return;
    const semanticItems = items.filter((x) => x.kind === 'semantic');
    if (semanticItems.length === 0) {
      setBatchNote('无语义冲突可批量处理（副本冲突请逐条决策）');
      return;
    }
    setBatchBusy(true);
    setBatchNote(null);
    let ok = 0;
    let failed = 0;
    const runNext = (idx: number) => {
      if (idx >= semanticItems.length) {
        setBatchBusy(false);
        setBatchNote(
          `批量完成：最新写入优先 ${ok} 条${failed > 0 ? ` · 失败 ${failed} 条（见行内错误）` : ''}`,
        );
        onChanged?.();
        return;
      }
      const it = semanticItems[idx];
      invoke('cmd_conflict_resolve_semantic', {
        conflict_id: it.id,
        resolution: 'LastWriteWins',
      })
        .then(() => {
          ok += 1;
          setItems((list) => (list ? list.filter((x) => x.id !== it.id) : list));
          runNext(idx + 1);
        })
        .catch(() => {
          failed += 1;
          setRowErr((m) => ({ ...m, [it.id]: '批量解决失败（最新写入优先）' }));
          runNext(idx + 1);
        });
    };
    runNext(0);
  }, [items, invoke, onChanged]);

  const pending = items ?? [];
  const semanticCount = pending.filter((x) => x.kind === 'semantic').length;

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(8, 12, 18, 0.72)',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        zIndex: 1000,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="冲突处理"
        style={{
          width: 680,
          maxWidth: '94vw',
          maxHeight: '80vh',
          display: 'flex',
          flexDirection: 'column',
          background: tokens.color.bgSurface,
          borderRadius: tokens.radius.lg,
          border: '1px solid rgba(55, 220, 242, 0.25)',
          boxShadow: '0 24px 64px rgba(0,0,0,0.5)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: tokens.spacing.sm,
            padding: tokens.spacing.md,
            borderBottom: '1px solid rgba(255,255,255,0.08)',
          }}
        >
          <strong style={{ fontSize: tokens.typography.heading.size }}>
            冲突处理{items ? `（待处理 ${pending.length}）` : ''}
          </strong>
          <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
            <button
              type="button"
              onClick={resolveAllLww}
              disabled={batchBusy || semanticCount === 0}
              title={
                semanticCount === 0
                  ? '无语义冲突（副本冲突不支持批量 LWW，请逐条决策）'
                  : `对 ${semanticCount} 条语义冲突逐条执行最新写入优先`
              }
              style={{
                ...btnStyle,
                opacity: batchBusy || semanticCount === 0 ? 0.5 : 1,
                cursor: batchBusy || semanticCount === 0 ? 'not-allowed' : 'pointer',
              }}
            >
              {batchBusy ? '批量处理中…' : `全部按最新写入优先（${semanticCount}）`}
            </button>
            <button type="button" onClick={onClose} style={btnStyle} aria-label="关闭">
              关闭
            </button>
          </div>
        </div>
        <div style={{ padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`, overflowY: 'auto' }}>
          {loadErr && (
            <div style={{ color: tokens.color.danger, padding: tokens.spacing.md }}>
              冲突查询失败：{loadErr}
            </div>
          )}
          {items !== null && pending.length === 0 && (
            <div
              style={{
                padding: `${tokens.spacing.xl}px ${tokens.spacing.md}px`,
                textAlign: 'center',
                color: tokens.color.textSecondary,
              }}
            >
              无待处理冲突
            </div>
          )}
          {batchNote && (
            <div
              role="status"
              style={{
                marginBottom: tokens.spacing.sm,
                padding: tokens.spacing.sm,
                background: tokens.color.bgElevated,
                borderRadius: tokens.radius.md,
                color: tokens.color.textSecondary,
              }}
            >
              {batchNote}
            </div>
          )}
          {pending.map((it) => (
            <div
              key={it.id}
              style={{
                background: tokens.color.bgElevated,
                border: '1px solid rgba(255,255,255,0.08)',
                borderRadius: tokens.radius.md,
                padding: tokens.spacing.md,
                marginBottom: tokens.spacing.sm,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: tokens.spacing.sm }}>
                <span
                  title={it.kind === 'artifact' ? '真冲突副本' : '语义冲突'}
                  style={{
                    background:
                      it.kind === 'artifact'
                        ? 'rgba(232, 93, 93, 0.16)'
                        : 'rgba(232, 179, 61, 0.16)',
                    color: it.kind === 'artifact' ? tokens.color.danger : tokens.color.warning,
                    borderRadius: 999,
                    padding: '1px 8px',
                    fontSize: 11,
                    fontWeight: 600,
                  }}
                >
                  {it.kind === 'artifact' ? '副本' : '语义'}
                </span>
                <span style={{ fontFamily: tokens.typography.mono, fontSize: 12 }} title={it.id}>
                  {shortId(it.id)}
                </span>
                <span style={{ color: tokens.color.textSecondary, fontSize: 12 }}>
                  {it.created_at}
                </span>
              </div>
              <div style={{ margin: `${tokens.spacing.xs}px 0 ${tokens.spacing.sm}px` }}>
                <span style={{ color: tokens.color.textSecondary }}>文档 </span>
                <span style={{ fontFamily: tokens.typography.mono, fontSize: 12 }}>
                  {it.doc_id}
                </span>
                <span style={{ color: tokens.color.textSecondary }}> · </span>
                <span title={it.kind === 'artifact' ? '远端路径' : '冲突字段'}>{it.summary}</span>
              </div>
              <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
                <button
                  type="button"
                  disabled={it.kind !== 'artifact' || busyId === it.id}
                  title={
                    it.kind === 'artifact'
                      ? '人工调和完成，副本已留存远端路径（状态闭环）'
                      : '副本留存仅适用于真冲突副本'
                  }
                  onClick={() => resolveOne(it, 'keep')}
                  style={{
                    ...btnStyle,
                    opacity: it.kind !== 'artifact' ? 0.4 : 1,
                    cursor: it.kind !== 'artifact' ? 'not-allowed' : 'pointer',
                  }}
                >
                  {busyId === it.id ? '处理中…' : '副本留存'}
                </button>
                <button
                  type="button"
                  disabled={it.kind !== 'semantic' || busyId === it.id}
                  title={
                    it.kind === 'semantic'
                      ? '按最新写入时间取舍（LastWriteWins）'
                      : '最新写入优先仅适用于语义冲突'
                  }
                  onClick={() => resolveOne(it, 'lww')}
                  style={{
                    ...btnStyle,
                    opacity: it.kind !== 'semantic' ? 0.4 : 1,
                    cursor: it.kind !== 'semantic' ? 'not-allowed' : 'pointer',
                  }}
                >
                  {busyId === it.id ? '处理中…' : '最新写入优先'}
                </button>
              </div>
              {rowErr[it.id] && (
                <div
                  role="alert"
                  style={{ marginTop: tokens.spacing.xs, color: tokens.color.danger, fontSize: 12 }}
                >
                  解决失败：{rowErr[it.id]}
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

const btnStyle: React.CSSProperties = {
  background: tokens.color.bgSurface,
  border: '1px solid rgba(255,255,255,0.10)',
  borderRadius: tokens.radius.md,
  padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
  color: tokens.color.textPrimary,
  fontSize: tokens.typography.caption.size,
};
