/**
 * DK-2.5 标签面板 v1 — 全库标签聚合 + 组合过滤。
 *
 * - 数据：cmd_list_tags() → [{tag, count}]（聚合计数，trash/Folder/
 *   SmartFolder 排除；计数降序）→ cmd_notes_by_tags(include, exclude)
 *   组合过滤（include=AND / exclude=NOT——FilterRule 同语义）；
 * - 交互：标签行点击 = include 切换（多选 AND）；已选中行 × = 加入
 *   exclude（NOT）；清除按钮重置；分层标签（`a/b`）按层级缩进呈现；
 * - 写通路：本视图零新写面（编辑复用 cmd_set_note_tags——EditorPane
 *   侧通道；v1 边界：标签重命名/合并/删除延后）；
 * - 呈现纪律：文本原样渲染（不可信内容零 HTML interpret）；invoke=null
 *   （browser-mock）或失败 → 空态降级。
 */
import React, { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
type TagCount = { tag: string; count: number };
type TaggedNote = { noteId: string; title: string | null; tags: string[] };

export default function TagView(props: {
  invoke: InvokeFn | null;
  onOpenNote: (id: string) => void;
}): React.ReactElement {
  const { invoke, onOpenNote } = props;
  const [tags, setTags] = useState<TagCount[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [include, setInclude] = useState<string[]>([]);
  const [exclude, setExclude] = useState<string[]>([]);
  const [hits, setHits] = useState<TaggedNote[] | null>(null);

  const reloadTags = useCallback(
    (alive: { current: boolean }) => {
      if (!invoke) {
        setTags([]);
        return;
      }
      invoke('cmd_list_tags')
        .then((rows) => {
          if (alive.current) setTags((rows as TagCount[]) ?? []);
        })
        .catch(() => {
          if (alive.current) {
            setTags([]);
            setFailed(true);
          }
        });
    },
    [invoke],
  );

  useEffect(() => {
    const alive = { current: true };
    reloadTags(alive);
    return () => {
      alive.current = false;
    };
  }, [reloadTags]);

  /* 组合过滤：include/exclude 变化即拉取（空+空 = 不拉） */
  useEffect(() => {
    if (!invoke || (include.length === 0 && exclude.length === 0)) {
      setHits(null);
      return;
    }
    const alive = { current: true };
    invoke('cmd_notes_by_tags', { include, exclude })
      .then((rows) => {
        if (alive.current) setHits((rows as TaggedNote[]) ?? []);
      })
      .catch(() => {
        if (alive.current) setHits([]);
      });
    return () => {
      alive.current = false;
    };
  }, [invoke, include, exclude]);

  const toggleInclude = (tag: string): void => {
    setInclude((prev) => (prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag]));
    setExclude((prev) => prev.filter((t) => t !== tag)); // 互斥（include 优先）
  };
  const toggleExclude = (tag: string): void => {
    setExclude((prev) => (prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag]));
    setInclude((prev) => prev.filter((t) => t !== tag));
  };

  const row = (t: TagCount): React.ReactElement => {
    const depth = t.tag.split('/').length - 1;
    const isIn = include.includes(t.tag);
    const isEx = exclude.includes(t.tag);
    return (
      <li
        key={t.tag}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '5px 4px 5px ' + (4 + depth * 14) + 'px',
          borderBottom: '1px solid rgba(255,255,255,0.05)',
          cursor: 'pointer',
          background: isIn ? 'rgba(60,120,255,0.14)' : isEx ? 'rgba(255,80,80,0.10)' : 'transparent',
          textDecoration: isEx ? 'line-through' : 'none',
        }}
        onClick={() => toggleInclude(t.tag)}
        title={isIn ? '移出过滤（AND）' : '加入过滤（AND 语义）'}
      >
        <span style={{ flex: 1, fontSize: tokens.typography.caption.size + 1, overflow: 'hidden', textOverflow: 'ellipsis' }}>
          #{t.tag}
        </span>
        <span style={{ fontSize: 11, color: tokens.color.textSecondary, flexShrink: 0 }}>{t.count}</span>
        <span
          role="button"
          aria-label={`排除标签：${t.tag}`}
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              e.stopPropagation();
              toggleExclude(t.tag);
            }
          }}
          onClick={(e) => {
            e.stopPropagation();
            toggleExclude(t.tag);
          }}
          style={{
            fontSize: 10,
            color: isEx ? tokens.color.danger : tokens.color.textDisabled,
            border: `1px solid ${isEx ? tokens.color.danger : 'rgba(255,255,255,0.14)'}`,
            borderRadius: 3,
            padding: '0 4px',
            flexShrink: 0,
          }}
        >
          ✕
        </span>
      </li>
    );
  };

  const open = tags === null;
  const active = include.length > 0 || exclude.length > 0;

  return (
    <main style={{ flex: 1, padding: tokens.spacing.lg, overflowY: 'auto', display: 'flex', gap: tokens.spacing.lg }}>
      <section style={{ flex: 1, minWidth: 0 }}>
        <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size }}>标签</h1>
        <p style={{ margin: `0 0 ${tokens.spacing.md}px`, color: tokens.color.textSecondary, fontSize: 12 }}>
          {failed ? '加载失败（已降级）' : open ? '加载中…' : `${(tags ?? []).length} 个标签 · 点击=包含（AND）· ✕=排除（NOT）`}
        </p>
        {!open && (tags ?? []).length === 0 && (
          <p style={{ color: tokens.color.textSecondary }}>
            全库暂无标签（EditorPane 标签编辑入口设置——导入的 Front matter 标签下次快照同步）
          </p>
        )}
        <ul style={{ listStyle: 'none', margin: 0, padding: 0, maxWidth: 480 }}>{(tags ?? []).map(row)}</ul>
      </section>
      <section style={{ flex: 1, minWidth: 0 }}>
        <h2 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size - 2 }}>
          过滤结果
          {active && (
            <button
              onClick={() => {
                setInclude([]);
                setExclude([]);
              }}
              style={{
                marginLeft: 10,
                fontSize: 11,
                border: '1px solid rgba(255,255,255,0.14)',
                background: 'transparent',
                color: tokens.color.textSecondary,
                borderRadius: tokens.radius.sm,
                cursor: 'pointer',
                padding: '2px 8px',
              }}
            >
              清除（{include.length} 含 / {exclude.length} 斥）
            </button>
          )}
        </h2>
        {!active && <p style={{ color: tokens.color.textSecondary }}>左侧选择标签组合过滤（include AND · exclude NOT）</p>}
        {active && hits === null && <p style={{ color: tokens.color.textSecondary }}>检索中…</p>}
        {active && hits !== null && hits.length === 0 && (
          <p style={{ color: tokens.color.textSecondary }}>无命中笔记</p>
        )}
        <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
          {(hits ?? []).map((n) => (
            <li
              key={n.noteId}
              onClick={() => onOpenNote(n.noteId)}
              style={{
                padding: '6px 4px',
                borderBottom: '1px solid rgba(255,255,255,0.05)',
                cursor: 'pointer',
              }}
              title="打开笔记"
            >
              <div style={{ fontSize: tokens.typography.caption.size + 2 }}>{n.title ?? '（未命名）'}</div>
              <div style={{ fontSize: 11, color: tokens.color.textSecondary, marginTop: 2 }}>
                {(n.tags ?? []).map((t) => `#${t}`).join(' ')}
              </div>
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
