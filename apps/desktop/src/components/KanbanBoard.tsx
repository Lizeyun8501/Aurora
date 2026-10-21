/**
 * G4：看板视图 — 投影任务四列（待办 inbox / 进行 next / 等待 waiting+
 * scheduled+someday / 完成 done+cancelled），HTML5 拖拽换列。
 *
 * 数据：cmd_board_tasks（投影全量行——含 ai:* 播种行）；
 * 换列：cmd_set_task_status（core set_row_status——DK-25 stamp 语义与
 * 事件路径零分叉）。
 *
 * v1 诚实边界：
 * - 不回写笔记文本（R2-Bug1 title 映射保投影态自洽——保存重建时看板
 *   设置的 status 被映射保留；文本回写 v2）；
 * - 拖拽仅 desktop（无触屏 fallback——卡片双击循环切列 v2）。
 */
import React, { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface BoardTask {
  task_id: string;
  note_id: string;
  title: string;
  status: string;
  priority: string;
  due_date: number | null;
}

const COLUMNS: Array<{ key: string; label: string; statuses: string[]; tone: string }> = [
  { key: 'inbox', label: '待办', statuses: ['inbox'], tone: '#8ab8ff' },
  { key: 'next', label: '进行', statuses: ['next'], tone: '#3fb950' },
  { key: 'waiting', label: '等待', statuses: ['waiting', 'scheduled', 'someday'], tone: '#d29922' },
  { key: 'done', label: '完成', statuses: ['done', 'cancelled'], tone: '#8b949e' },
];

export default function KanbanBoard(props: {
  invoke: InvokeFn | null;
  onOpenNote: (id: string) => void;
}): React.ReactElement {
  const { invoke, onOpenNote } = props;
  const [tasks, setTasks] = useState<BoardTask[]>([]);
  const [failed, setFailed] = useState(false);
  const [dragId, setDragId] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState<string | null>(null);

  const reload = useCallback(() => {
    if (!invoke) return;
    invoke('cmd_board_tasks')
      .then((rows) => setTasks((rows as BoardTask[]) ?? []))
      .catch(() => setFailed(true));
  }, [invoke]);

  useEffect(() => {
    reload();
  }, [reload]);

  const drop = (colKey: string): void => {
    const id = dragId;
    setDragId(null);
    setDragOver(null);
    if (!invoke || !id) return;
    const target = COLUMNS.find((c) => c.key === colKey);
    const t = tasks.find((x) => x.task_id === id);
    if (!target || !t || target.statuses.includes(t.status)) return;
    // 乐观移动 → 服务端 → 失败回滚重拉
    setTasks((prev) =>
      prev.map((x) => (x.task_id === id ? { ...x, status: target.statuses[0] } : x)),
    );
    invoke('cmd_set_task_status', { taskId: id, status: target.statuses[0] }).catch(() => reload());
  };

  if (!invoke) {
    return <p style={{ color: tokens.color.textSecondary }}>browser-mock 模式——看板仅 tauri 模式可用</p>;
  }
  if (failed) {
    return <p style={{ color: tokens.color.textSecondary }}>看板加载失败（投影未注册？）——<button onClick={reload} style={{ color: tokens.color.primaryBright, background: 'none', border: 'none', cursor: 'pointer' }}>重试</button></p>;
  }

  return (
    <div style={{ display: 'flex', gap: tokens.spacing.sm, alignItems: 'flex-start', overflowX: 'auto' }}>
      {COLUMNS.map((col) => {
        const list = tasks.filter((t) => col.statuses.includes(t.status));
        return (
          <div
            key={col.key}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(col.key);
            }}
            onDragLeave={() => setDragOver((k) => (k === col.key ? null : k))}
            onDrop={() => drop(col.key)}
            style={{
              flex: 1,
              minWidth: 170,
              background: dragOver === col.key ? 'rgba(88,166,255,0.08)' : tokens.color.bgSurface,
              border: `1px solid ${dragOver === col.key ? 'rgba(88,166,255,0.45)' : 'rgba(255,255,255,0.07)'}`,
              borderRadius: tokens.radius.md,
              padding: tokens.spacing.sm,
              minHeight: 180,
              transition: 'background 120ms',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: tokens.spacing.sm }}>
              <span style={{ fontSize: 13, fontWeight: 600, color: col.tone }}>{col.label}</span>
              <span
                style={{
                  fontSize: 11,
                  padding: '0 7px',
                  borderRadius: 999,
                  background: 'rgba(255,255,255,0.08)',
                  color: tokens.color.textSecondary,
                }}
              >
                {list.length}
              </span>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.xs }}>
              {list.map((t) => (
                <div
                  key={t.task_id}
                  draggable
                  onDragStart={() => setDragId(t.task_id)}
                  onDragEnd={() => {
                    setDragId(null);
                    setDragOver(null);
                  }}
                  onClick={() => onOpenNote(t.note_id)}
                  title="拖拽换列 · 点击打开源笔记"
                  style={{
                    background: tokens.color.bgElevated,
                    border: `1px solid ${dragId === t.task_id ? 'rgba(88,166,255,0.6)' : 'rgba(255,255,255,0.08)'}`,
                    borderRadius: tokens.radius.sm,
                    padding: `${tokens.spacing.xs + 1}px ${tokens.spacing.sm}px`,
                    cursor: 'grab',
                    opacity: dragId === t.task_id ? 0.5 : 1,
                  }}
                >
                  <div style={{ fontSize: 13, color: tokens.color.textPrimary, marginBottom: 2 }}>
                    {t.title}
                  </div>
                  <div style={{ fontSize: 11, color: tokens.color.textSecondary, display: 'flex', gap: 6 }}>
                    {t.priority !== 'medium' && <span>{t.priority}</span>}
                    {t.due_date != null && <span>📅 {new Date(t.due_date).toLocaleDateString()}</span>}
                  </div>
                </div>
              ))}
              {list.length === 0 && (
                <div style={{ fontSize: 12, color: tokens.color.textSecondary, textAlign: 'center', padding: '14px 0' }}>
                  空
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
