/**
 * DK-02 S1 回收站视图（Alpha UI 切片）——core 面见 bravo f175692。
 *
 * - 恢复 = NoteCreated 重放驱动投影/索引重建（Bravo 裁决：投影禁改零触碰）；
 * - 彻底删除 = purge（不可逆——confirm 门 + 附件级联回归）；
 * - browser-mock 模式（invoke = null）显示提示态（core 命令仅 tauri 模式可用）。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

interface TrashItem {
  note_id: string;
  deleted_at_ms: number;
  title: string;
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

export default function TrashView({ invoke }: { invoke: InvokeFn | null }): React.ReactElement {
  const [items, setItems] = useState<TrashItem[] | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    if (!invoke) return;
    invoke('cmd_list_trashed')
      .then((r) => {
        setItems(r as TrashItem[]);
        setError(null);
      })
      .catch((e) => {
        setItems(null);
        setError(String(e));
      });
  }, [invoke]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const restore = useCallback(
    (it: TrashItem) => {
      if (!invoke || busyId) return;
      setBusyId(it.note_id);
      invoke('cmd_restore_note', { noteId: it.note_id })
        .then(() => refresh())
        .catch((e) => setError(String(e)))
        .finally(() => setBusyId(null));
    },
    [invoke, busyId, refresh],
  );

  const purge = useCallback(
    (it: TrashItem) => {
      if (!invoke || busyId) return;
      if (!window.confirm(`彻底删除「${it.title}」？此操作不可恢复。`)) return;
      setBusyId(it.note_id);
      invoke('cmd_purge_note', { noteId: it.note_id })
        .then(() => refresh())
        .catch((e) => setError(String(e)))
        .finally(() => setBusyId(null));
    },
    [invoke, busyId, refresh],
  );

  return (
    <main style={{ flex: 1, padding: tokens.spacing.lg, overflowY: 'auto' }} aria-label="回收站">
      <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>
        回收站
      </h1>
      {invoke && (
        <div style={{ display: 'flex', alignItems: 'center', gap: tokens.spacing.sm, marginBottom: tokens.spacing.sm }}>
          <button
            style={btn}
            disabled={busyId !== null}
            onClick={async () => {
              setBusyId('__purge__');
              try {
                const out = (await invoke('cmd_purge_expired_trash', { days: 30 })) as string[];
                alert(`已清理 ${out.length} 条超期回收站项（30 天前）`);
                await refresh();
              } catch (e) {
                setError(String(e));
              }
              setBusyId(null);
            }}
          >
            清理 30 天前过期项
          </button>
          <span style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
            DK-17：超期标记全 purge（物理删，不可逆）
          </span>
        </div>
      )}
      {!invoke ? (
        <p style={{ color: tokens.color.textSecondary }} role="status">
          回收站需要桌面端模式（tauri）。
        </p>
      ) : error !== null ? (
        <p style={{ color: tokens.color.warning }} role="alert">
          回收站加载失败：{error}
        </p>
      ) : items !== null && items.length === 0 ? (
        <p style={{ color: tokens.color.textSecondary }} role="status">
          回收站为空。
        </p>
      ) : items === null ? (
        <p style={{ color: tokens.color.textSecondary }} role="status">
          加载中…
        </p>
      ) : (
        <ul role="list" style={{ listStyle: 'none', margin: 0, padding: 0, display: 'flex', flexDirection: 'column', gap: tokens.spacing.xs }}>
          {items.map((it) => (
            <li
              key={it.note_id}
              role="listitem"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: tokens.spacing.sm,
                padding: tokens.spacing.sm,
                background: tokens.color.bgElevated,
                borderRadius: tokens.radius.md,
                border: '1px solid rgba(255,255,255,0.08)',
              }}
            >
              <span style={{ flex: 1, color: tokens.color.textPrimary, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {it.title || '（无标题）'}
              </span>
              <span style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
                {new Date(it.deleted_at_ms).toLocaleString()}
              </span>
              <button
                style={btn}
                onClick={() => restore(it)}
                disabled={busyId !== null}
                aria-label={`恢复笔记 ${it.title}`}
              >
                恢复
              </button>
              <button
                style={{ ...btn, color: tokens.color.warning }}
                onClick={() => purge(it)}
                disabled={busyId !== null}
                aria-label={`彻底删除笔记 ${it.title}`}
              >
                彻底删除
              </button>
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}
