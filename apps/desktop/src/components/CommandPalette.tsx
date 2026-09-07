/**
 * CommandPalette — V23-I1 桌面四件套之一（Mod+K）
 *
 * 数据源：Tauri command cmd_search_notes（jieba 中文分词 + Tantivy）
 * 与移动端 search_notes 同一 Rust 契约（desktop_mobile_view_model_isomorphic
 * 对拍保护）。
 *
 * 交互（V22.1 §6.6）：
 * - Mod+K 唤起（AppShell 全局监听）；Esc 关闭；↑↓ 选择；Enter 打开
 * - 空态引导：内置命令 + 口语化提示
 * - 口语化查询透传（「本周未完成的任务」→ NL 解析 → 投影聚合）
 */
import { useEffect, useRef, useState } from 'react';
import tokens from '../design/tokens';

export interface PaletteItem {
  kind: 'note' | 'command';
  id: string;
  title: string;
  snippet?: string;
  /** note → 打开笔记; command → 直接执行 */
  run: () => void;
}

interface Props {
  open: boolean;
  onClose: () => void;
  /** Tauri cmd_search_notes 桥（AppShell 注入） */
  searchNotes: (query: string) => Promise<
    Array<{ note_id: string; title: string; snippet: string; score: number }>
  >;
  /** 输入变化回调（AppShell 合并内置命令与搜索结果） */
  onQuery: (query: string) => void;
  /** AppShell 组装好的条目（内置命令 + 搜索结果） */
  items: PaletteItem[];
  openNote: (noteId: string) => void;
}

export function CommandPalette({ open, onClose, searchNotes, onQuery, items, openNote }: Props) {
  const [query, setQuery] = useState('');
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // 唤起时聚焦 + 重置游标
  useEffect(() => {
    if (open) {
      setCursor(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  // 防抖 150ms 搜索（击键 → cmd_search_notes）
  useEffect(() => {
    if (!open || !query.trim()) return;
    const t = setTimeout(() => {
      void searchNotes(query).then(() => onQuery(query));
    }, 150);
    return () => clearTimeout(t);
  }, [query, open, searchNotes, onQuery]);

  if (!open) return null;

  const run = (item: PaletteItem) => {
    item.run();
    if (item.kind === 'note') openNote(item.id);
    onClose();
    setQuery('');
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(8, 12, 18, 0.72)',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'flex-start',
        paddingTop: '12vh',
        zIndex: 1000,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="命令面板"
        style={{
          width: 560,
          maxWidth: '92vw',
          background: tokens.color.bgSurface,
          borderRadius: tokens.radius.lg,
          border: `1px solid rgba(55, 220, 242, 0.25)`,
          boxShadow: '0 24px 64px rgba(0,0,0,0.5)',
          overflow: 'hidden',
        }}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setCursor(0);
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              setCursor((c) => Math.min(c + 1, items.length - 1));
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              setCursor((c) => Math.max(c - 1, 0));
            } else if (e.key === 'Enter' && items[cursor]) {
              run(items[cursor]);
            }
          }}
          placeholder="搜索笔记，或键入命令名 — 口语化可用（如「未完成的任务」）"
          aria-label="搜索与命令"
          style={{
            width: '100%',
            boxSizing: 'border-box',
            background: tokens.color.bgElevated,
            border: 'none',
            borderBottom: '1px solid rgba(255,255,255,0.08)',
            padding: `${tokens.spacing.md}px`,
            color: tokens.color.textPrimary,
            fontSize: tokens.typography.body.size,
            outline: 'none',
          }}
        />
        <div style={{ maxHeight: 380, overflowY: 'auto' }}>
          {items.length === 0 && (
            <div style={{ padding: tokens.spacing.lg, color: tokens.color.textDisabled, textAlign: 'center' }}>
              无匹配 — 支持口语化：「本周未完成的任务」「关于 某主题」
            </div>
          )}
          {items.map((r, i) => (
            <div
              key={`${r.kind}:${r.id}`}
              onClick={() => run(r)}
              onMouseEnter={() => setCursor(i)}
              role="option"
              aria-selected={i === cursor}
              style={{
                padding: `${tokens.spacing.sm + 2}px ${tokens.spacing.md}px`,
                cursor: 'pointer',
                background: i === cursor ? tokens.color.primary : 'transparent',
                color: i === cursor ? '#FFFFFF' : tokens.color.textPrimary,
                borderBottom: '1px solid rgba(255,255,255,0.06)',
              }}
            >
              <div style={{ fontWeight: r.kind === 'command' ? 600 : 400, fontSize: tokens.typography.body.size }}>
                {r.kind === 'command' ? '⌘ ' : '📄 '}
                {r.title}
              </div>
              {r.snippet && (
                <div
                  style={{
                    fontSize: tokens.typography.caption.size,
                    color: i === cursor ? '#DDE8EC' : tokens.color.textSecondary,
                    marginTop: 2,
                  }}
                >
                  {r.snippet}
                </div>
              )}
            </div>
          ))}
        </div>
        <div
          style={{
            padding: tokens.spacing.sm,
            color: tokens.color.textDisabled,
            fontSize: tokens.typography.caption.size,
            borderTop: '1px solid rgba(255,255,255,0.08)',
          }}
        >
          ↑↓ 选择 · Enter 打开 · Esc 关闭 · 口语化可用（如「未完成的任务」）
        </div>
      </div>
    </div>
  );
}

export default CommandPalette;
