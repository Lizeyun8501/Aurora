/**
 * DK-2.2 待办聚合 v2 — 全库 task 块跨笔记聚合 + checkbox 回写。
 *
 * - 数据：cmd_all_tasks() → [{noteId, noteTitle, done, text, blockId,
 *   blockPosition}]（core BlockStore.list_blocks_by_type("task") 单条 SQL
 *   + trash 排除 + 标题恒明文补全；未完成在前）；
 * - 交互 v2：checkbox 可点——cmd_toggle_task(noteId, blockId, done) 回写
 *   源笔记（双条件块定位+行级切换+全纪律 save）；乐观更新+失败回滚+聚合
 *   重拉；行点击仍跳源笔记（checkbox stopPropagation 防误跳）；
 * - 呈现纪律：文本原样渲染（不可信内容零 HTML interpret）；invoke=null
 *   （browser-mock）或失败 → 空态降级。
 */
import React, { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
type TaskItem = {
  noteId: string;
  noteTitle: string | null;
  done: boolean;
  text: string;
  blockId: string;
  blockPosition: number;
};

export default function TodoView(props: {
  invoke: InvokeFn | null;
  onOpenNote: (id: string) => void;
  /** DK-3.1：右侧常驻形态——缩 padding/去大标题/限高滚动。 */
  compact?: boolean;
}): React.ReactElement {
  const { invoke, onOpenNote, compact = false } = props;
  const [items, setItems] = useState<TaskItem[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  const reload = useCallback(
    (alive: { current: boolean }) => {
      if (!invoke) {
        setItems([]);
        return;
      }
      invoke('cmd_all_tasks')
        .then((rows) => {
          if (alive.current) setItems((rows as TaskItem[]) ?? []);
        })
        .catch(() => {
          if (alive.current) {
            setItems([]);
            setFailed(true);
          }
        });
    },
    [invoke],
  );

  useEffect(() => {
    const alive = { current: true };
    setItems(null);
    setFailed(false);
    reload(alive);
    return () => {
      alive.current = false;
    };
  }, [reload]);

  /** v2 回写：乐观切换 → 服务端回写 → 失败回滚 + 重拉对齐。 */
  const toggle = (t: TaskItem): void => {
    if (!invoke || busy) return;
    const key = `${t.noteId}:${t.blockId}`;
    const target = !t.done;
    setBusy(key);
    setItems((prev) =>
      (prev ?? []).map((x) => (x.noteId === t.noteId && x.blockId === t.blockId ? { ...x, done: target } : x)),
    );
    invoke('cmd_toggle_task', { noteId: t.noteId, blockId: t.blockId, done: target })
      .then(() => {
        const alive = { current: true };
        reload(alive); // 回写后重拉（块 id 随整篇 hash 变——对齐新快照）
      })
      .catch(() => {
        setItems((prev) =>
          (prev ?? []).map((x) => (x.noteId === t.noteId && x.blockId === t.blockId ? { ...x, done: !target } : x)),
        );
      })
      .finally(() => setBusy(null));
  };

  const open = items === null;
  const pending = (items ?? []).filter((t) => !t.done);
  const done = (items ?? []).filter((t) => t.done);

  const row = (t: TaskItem, isDone: boolean): React.ReactElement => {
    const key = `${t.noteId}:${t.blockId}`;
    return (
      <li
        key={key}
        style={{
          display: 'flex',
          alignItems: 'baseline',
          gap: 8,
          padding: '6px 4px',
          borderBottom: '1px solid rgba(255,255,255,0.05)',
          cursor: 'pointer',
        }}
        onClick={() => onOpenNote(t.noteId)}
        title="打开源笔记"
      >
        <span
          role="checkbox"
          aria-checked={isDone}
          aria-label={`切换任务：${t.text}`}
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              toggle(t);
            }
          }}
          onClick={(e) => {
            e.stopPropagation();
            toggle(t);
          }}
          style={{
            display: 'inline-block',
            width: 14,
            height: 14,
            borderRadius: 3,
            border: `1px solid ${isDone ? tokens.color.success : tokens.color.textDisabled}`,
            background: isDone ? tokens.color.success : 'transparent',
            flexShrink: 0,
            position: 'relative',
            top: 2,
            cursor: busy === key ? 'wait' : 'pointer',
            opacity: busy === key ? 0.5 : 1,
          }}
        />
        <span
          style={{
            flex: 1,
            color: isDone ? tokens.color.textDisabled : tokens.color.textPrimary,
            textDecoration: isDone ? 'line-through' : 'none',
            fontSize: tokens.typography.caption.size + 1,
          }}
        >
          {t.text || '（空任务行）'}
        </span>
        <span style={{ fontSize: 11, color: tokens.color.textSecondary, flexShrink: 0 }}>
          {t.noteTitle ?? '（未命名笔记）'}
        </span>
      </li>
    );
  };

  return (
    <main
      style={{
        flex: 1,
        padding: compact ? tokens.spacing.sm : tokens.spacing.lg,
        overflowY: 'auto',
        maxHeight: compact ? 320 : undefined,
      }}
    >
      {!compact && (
        <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>
          待办聚合
        </h1>
      )}
      <p style={{ margin: `0 0 ${tokens.spacing.md}px`, color: tokens.color.textSecondary, fontSize: 12 }}>
        {failed
          ? '加载失败（已降级）'
          : open
            ? '加载中…'
            : `进行中 ${pending.length} · 已完成 ${done.length}（勾选直接回写源笔记）`}
      </p>
      {!open && !failed && (items ?? []).length === 0 && (
        <p style={{ color: tokens.color.textSecondary }}>全库暂无任务行（`- [ ] ` 开头即计入）</p>
      )}
      {!open && pending.length > 0 && (
        <>
          <h2 style={{ fontSize: tokens.typography.caption.size + 1, color: tokens.color.warning, margin: `0 0 4px` }}>
            进行中
          </h2>
          <ul style={{ listStyle: 'none', margin: `0 0 ${tokens.spacing.md}px`, padding: 0 }}>
            {pending.map((t) => row(t, false))}
          </ul>
        </>
      )}
      {!open && done.length > 0 && (
        <>
          <h2 style={{ fontSize: tokens.typography.caption.size + 1, color: tokens.color.success, margin: `0 0 4px` }}>
            已完成
          </h2>
          <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
            {done.map((t) => row(t, true))}
          </ul>
        </>
      )}
    </main>
  );
}
