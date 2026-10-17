/**
 * DK-2.2 待办聚合 v1（只读）— 全库 task 块跨笔记聚合视图。
 *
 * - 数据：cmd_all_tasks() → [{noteId, noteTitle, done, text}]（core
 *   BlockStore.list_blocks_by_type("task") 单条 SQL + trash 排除 + 标题
 *   恒明文补全；未完成在前）；
 * - 交互 v1：点击任务跳转源笔记（onOpenNote 路由链复用）；**checkbox
 *   切换回写延后 v2**（需块定位+内容回写——诚实边界，只读展示不误导）；
 * - 呈现纪律：文本原样渲染（不可信内容零 HTML interpret）；invoke=null
 *   （browser-mock）或失败 → 空态降级。
 */
import React, { useEffect, useState } from 'react';
import tokens from '../design/tokens';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
type TaskItem = { noteId: string; noteTitle: string | null; done: boolean; text: string };

export default function TodoView(props: {
  invoke: InvokeFn | null;
  onOpenNote: (id: string) => void;
}): React.ReactElement {
  const { invoke, onOpenNote } = props;
  const [items, setItems] = useState<TaskItem[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    if (!invoke) {
      setItems([]);
      return;
    }
    setItems(null);
    setFailed(false);
    invoke('cmd_all_tasks')
      .then((rows) => {
        if (alive) setItems((rows as TaskItem[]) ?? []);
      })
      .catch(() => {
        if (alive) {
          setItems([]);
          setFailed(true);
        }
      });
    return () => {
      alive = false;
    };
  }, [invoke]);

  const open = items === null;
  const pending = (items ?? []).filter((t) => !t.done);
  const done = (items ?? []).filter((t) => t.done);

  const row = (t: TaskItem, isDone: boolean): React.ReactElement => (
    <li
      key={`${t.noteId}:${t.text}`}
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
        }}
        aria-hidden
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

  return (
    <main style={{ flex: 1, padding: tokens.spacing.lg, overflowY: 'auto' }}>
      <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>
        待办聚合
      </h1>
      <p style={{ margin: `0 0 ${tokens.spacing.md}px`, color: tokens.color.textSecondary, fontSize: 12 }}>
        {failed
          ? '加载失败（已降级）'
          : open
            ? '加载中…'
            : `进行中 ${pending.length} · 已完成 ${done.length}（v1 只读——勾选回写随块编辑卡）`}
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
