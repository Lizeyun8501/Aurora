/**
 * ConflictReviewPanel — DK-50 组件 3：冲突逐条决策（移植自桌面 DK-48）
 *
 * 逻辑层照搬（合并视图 / kind 分型分接口 / artifact 不适用 LWW 置灰
 * 防御延续 / 决策后本地移除局部刷新 / 批量 LWW 跳过 artifact 并计数 /
 * onChanged 联动）；形态=全屏底部 sheet，样式 mobile.css sync-* 前缀。
 *
 * 决策按钮按 kind 分派（DK-49 数据面形状）：artifact→conflictResolve
 * （副本留存闭环）；semantic→conflictResolveSemantic（最新写入优先）。
 * 反向组合置灰（resolve 仅收副本 id / resolveSemantic 仅收语义 id）。
 */
import { useCallback, useEffect, useState } from 'react';
import { platform, type SyncConflictItem } from '../adapters/androidPlatform';

interface Props {
  onClose: () => void;
  /** 决策成功后通知壳刷新（badge 冲突红点联动） */
  onChanged?: () => void;
}

export default function ConflictReviewPanel({ onClose, onChanged }: Props) {
  const [items, setItems] = useState<SyncConflictItem[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [rowErr, setRowErr] = useState<Record<string, string>>({});
  const [batchMsg, setBatchMsg] = useState<string | null>(null);

  const refresh = useCallback(() => {
    try {
      setItems(platform.conflictList());
    } catch {
      setItems([]);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  /** 决策成功 → 本地移除该条（不整页 reload）+ 通知壳联动 badge */
  const resolveItem = useCallback(
    (it: SyncConflictItem) => {
      setBusyId(it.id);
      setRowErr((m) => ({ ...m, [it.id]: '' }));
      try {
        const ok =
          it.kind === 'artifact'
            ? platform.conflictResolve(it.id)
            : platform.conflictResolveSemantic(it.id, 'LastWriteWins');
        if (ok) {
          setItems((list) => list.filter((x) => x.id !== it.id));
          onChanged?.();
        } else {
          setRowErr((m) => ({ ...m, [it.id]: '解决失败（桥返回未成功）' }));
        }
      } catch (e) {
        setRowErr((m) => ({ ...m, [it.id]: String(e) }));
      } finally {
        setBusyId(null);
      }
    },
    [onChanged],
  );

  /** 批量「全部按最新写入优先」：artifact 不适用 LWW——跳过并如实计数 */
  const batchLWW = useCallback(() => {
    const semantics = items.filter((x) => x.kind === 'semantic');
    const artifacts = items.length - semantics.length;
    let okCount = 0;
    const failures: string[] = [];
    for (const it of semantics) {
      try {
        if (platform.conflictResolveSemantic(it.id, 'LastWriteWins')) {
          okCount += 1;
        } else {
          failures.push(it.doc_id);
        }
      } catch {
        failures.push(it.doc_id);
      }
    }
    setItems((list) => list.filter((x) => !failures.includes(x.doc_id)));
    const parts = [`语义冲突 ${okCount}/${semantics.length} 已按最新写入优先`];
    if (artifacts > 0) parts.push(`副本冲突 ${artifacts} 条需逐条决策（不适用 LWW）`);
    if (failures.length > 0) parts.push(`失败 ${failures.length} 条`);
    setBatchMsg(parts.join(' · '));
    if (okCount > 0) onChanged?.();
  }, [items, onChanged]);

  return (
    <>
      <div className="sync-sheet-mask" onClick={onClose} />
      <div className="sync-sheet" role="dialog" aria-label="冲突处理">
        <div className="sync-sheet-grip" />
        <div className="sync-sheet-title">
          冲突处理{items.length > 0 ? `（待处理 ${items.length}）` : ''}
          <button type="button" className="sync-sheet-close" onClick={onClose}>
            关闭
          </button>
        </div>

        {items.length === 0 ? (
          <div className="sync-empty">无待处理冲突</div>
        ) : (
          <>
            {items.map((it) => (
              <div key={it.id} className="sync-item">
                <div>
                  <span className="sync-item-kind">{it.kind === 'artifact' ? '副本冲突' : '语义冲突'}</span>
                  <span className="sync-mono">{it.doc_id.length > 10 ? `${it.doc_id.slice(0, 10)}…` : it.doc_id}</span>
                </div>
                <div className="sync-item-sub">
                  {it.summary} · {it.created_at}
                </div>
                <div className="sync-btn-row">
                  <button
                    type="button"
                    className="sync-btn"
                    disabled={it.kind !== 'artifact' || busyId === it.id}
                    onClick={() => resolveItem(it)}
                  >
                    {busyId === it.id ? '处理中…' : '副本留存'}
                  </button>
                  <button
                    type="button"
                    className="sync-btn"
                    disabled={it.kind !== 'semantic' || busyId === it.id}
                    onClick={() => resolveItem(it)}
                  >
                    {busyId === it.id ? '处理中…' : '最新写入优先'}
                  </button>
                </div>
                {rowErr[it.id] && (
                  <div className="sync-item-err" role="alert">
                    解决失败：{rowErr[it.id]}
                  </div>
                )}
              </div>
            ))}
            <div className="sync-btn-row">
              <button type="button" className="sync-btn" onClick={batchLWW}>
                全部按最新写入优先
              </button>
            </div>
            {batchMsg && <div className="sync-item-sub">{batchMsg}</div>}
          </>
        )}
      </div>
    </>
  );
}
