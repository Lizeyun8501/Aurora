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
import { useEffect, useMemo, useRef, useState } from 'react';
import { fuzzySort } from './fuzzy';
import tokens from '../design/tokens';

export interface PaletteItem {
  kind: 'note' | 'command';
  id: string;
  title: string;
  snippet?: string;
  /** A-09 归一: bm25/vector/both（旧大写面兼容——渲染统一 lowercase） */
  source?: 'Bm25' | 'Vector' | 'Both' | 'bm25' | 'vector' | 'both';
  /** 仅 vector 命中的语义解释（「与『q』相近」——DoD 无来路不明命中） */
  semanticHint?: string | null;
  /** note → 打开笔记; command → 直接执行 */
  run: () => void;
}

interface Props {
  open: boolean;
  onClose: () => void;
  /** Tauri 桥（A-09: Shell 统一走 cmd_search_advanced;组件仅留存兼容签名） */
  searchNotes: (
    query: string,
    mode?: string,
  ) => Promise<unknown[]>;
  /** A-09 解析提示（运算符错误/回落文案——禁静默透出） */
  searchWarnings?: string[];
  /** 输入变化回调（A-09: 携语义开关——Shell runSearch 统一执行） */
  onQuery: (query: string, semantic?: boolean) => void;
  /** AppShell 组装好的条目（内置命令 + 搜索结果） */
  items: PaletteItem[];
  openNote: (noteId: string) => void;
}

export function CommandPalette({ open, onClose, searchNotes: _searchNotes, searchWarnings, onQuery, items, openNote }: Props) {
  // A-09: searchNotes 已收敛到 onQuery 单链（Shell runSearch 统一 cmd_search_advanced）;
  // prop 留兼容签名——旧注入面零破坏。
  const [query, setQuery] = useState('');
  const [cursor, setCursor] = useState(0);
  const [semantic, setSemantic] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  /* A-09: 最近使用查询（C-06 置顶——空 query 时建议区;localStorage Shell 写入） */
  const [recent, setRecent] = useState<string[]>([]);
  /* A-09: 运算符帮助浮层（palette 内置命令「搜索语法帮助」唤起——DoD 可检索） */
  const [helpOpen, setHelpOpen] = useState(false);
  useEffect(() => {
    if (open) {
      try {
        setRecent(JSON.parse(localStorage.getItem('aurora.recentQueries') ?? '[]') as string[]);
      } catch {
        setRecent([]);
      }
    }
  }, [open]);
  /* A-04: search 书签跳转（Shell onSearchJump → 本事件 → palette 预填 A-09 串
     ——保存的搜索书签复现原查询条件 DoD）。 */
  useEffect(() => {
    const onJump = (e: Event) => {
      const q = (e as CustomEvent<{ query: string }>).detail?.query ?? '';
      if (q) {
        setQuery(q);
        setTimeout(() => inputRef.current?.focus(), 0);
      }
    };
    window.addEventListener('aurora:search-jump', onJump);
    return () => window.removeEventListener('aurora:search-jump', onJump);
  }, []);

  // DK-3.2 切片一：模糊匹配过滤+排序（title 双路——命令与笔记统一打分；
  // 空 query 原样——内置命令全量展示）。语义/全文检索面不经此处
  // （results 由 cmd_search_notes 召回——模糊只作用于标题面，双路互补）。
  /* A-09: 内置「搜索语法帮助」命令（fuzzy 检索「语法/帮助/搜索」可命中——DoD） */
  const itemsWithHelp = useMemo(() => {
    if (items.some((i) => i.id === 'search-syntax-help')) return items;
    return [
      ...items,
      {
        kind: 'command' as const,
        id: 'search-syntax-help',
        title: '搜索语法帮助（path:/tag:/status:/type:/date: 与短语、排除词）',
        run: () => setHelpOpen((v) => !v),
      },
    ];
  }, [items]);
  const shown = useMemo(() => fuzzySort(itemsWithHelp, query, (i) => i.title), [itemsWithHelp, query]);

  // 唤起时聚焦 + 重置游标
  useEffect(() => {
    if (open) {
      setCursor(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  // 防抖 150ms 搜索（A-09: 单链——onQuery(q, semantic) 由 Shell runSearch 统一
  // 走 cmd_search_advanced;本组件不再直调 searchNotes（双调收敛,徽章面同源））
  useEffect(() => {
    if (!open || !query.trim()) return;
    const t = setTimeout(() => {
      onQuery(query, semantic);
    }, 150);
    return () => clearTimeout(t);
  }, [query, open, onQuery, semantic]);

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
        {/* DK-03 S3：语义检索开关（开→mode=hybrid，BM25+本地向量 RRF 融合） */}
        <label
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            padding: `0 ${tokens.spacing.md}px`,
            paddingTop: tokens.spacing.xs,
            fontSize: tokens.typography.caption.size,
            color: tokens.color.textSecondary,
            cursor: 'pointer',
            userSelect: 'none',
          }}
        >
          <input
            type="checkbox"
            checked={semantic}
            onChange={(e) => {
              setSemantic(e.target.checked);
              setCursor(0);
            }}
            aria-label="语义检索（本地向量混合）"
          />
          语义检索（本地向量 · Ollama 未运行时自动退回全文）
        </label>
        <div style={{ maxHeight: 380, overflowY: 'auto' }}>
          {shown.length === 0 && (
            <div style={{ padding: tokens.spacing.lg, color: tokens.color.textDisabled, textAlign: 'center' }}>
              无匹配 — 支持口语化：「本周未完成的任务」「关于 某主题」
            </div>
          )}
          {shown.map((r, i) => {
            const src = r.source?.toLowerCase();
            return (
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
                {src === 'vector' && (
                  <span
                    title={r.semanticHint ?? '语义召回：不含查询关键词，按含义匹配（本地向量）'}
                    style={{
                      marginLeft: 6,
                      padding: '1px 6px',
                      borderRadius: 999,
                      fontSize: 11,
                      background: 'rgba(167, 139, 250, 0.18)',
                      color: i === cursor ? '#FFFFFF' : '#A78BFA',
                      border: '1px solid rgba(167, 139, 250, 0.45)',
                    }}
                  >
                    语义相关
                  </span>
                )}
                {src === 'both' && (
                  <span
                    title="精确 + 语义：关键词与含义两路均命中（RRF 融合）"
                    style={{
                      marginLeft: 6,
                      padding: '1px 6px',
                      borderRadius: 999,
                      fontSize: 11,
                      background: 'rgba(56, 189, 248, 0.16)',
                      color: i === cursor ? '#FFFFFF' : '#38BDF8',
                      border: '1px solid rgba(56, 189, 248, 0.45)',
                    }}
                  >
                    精确 + 语义
                  </span>
                )}
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
          );
          })}
        </div>
        {/* A-09: 解析提示条（运算符错误/回落——禁静默;红字短文案） */}
        {(searchWarnings?.length ?? 0) > 0 && (
          <div
            role="status"
            style={{
              padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
              fontSize: tokens.typography.caption.size,
              color: '#E8B33D',
              borderTop: '1px solid rgba(232, 179, 61, 0.3)',
            }}
          >
            {searchWarnings!.slice(0, 2).map((w, i) => (
              <div key={i}>⚠ {w}</div>
            ))}
          </div>
        )}
        {/* A-09: 最近使用查询建议（C-06 置顶——空 query 显示;点击回填执行） */}
        {open && !query.trim() && recent.length > 0 && (
          <div style={{ padding: tokens.spacing.xs }}>
            <div style={{ fontSize: 11, color: tokens.color.textSecondary, padding: '2px 6px' }}>最近搜索</div>
            {recent.slice(0, 5).map((rq) => (
              <button
                key={rq}
                onClick={() => {
                  setQuery(rq);
                  onQuery(rq, semantic);
                }}
                style={{
                  display: 'block', width: '100%', textAlign: 'left',
                  padding: '4px 8px', background: 'transparent',
                  border: 'none', color: tokens.color.textPrimary,
                  cursor: 'pointer', fontSize: tokens.typography.caption.size + 1,
                  borderRadius: tokens.radius.sm,
                }}
              >
                ⏱ {rq}
              </button>
            ))}
          </div>
        )}
        {/* A-09: 帮助浮层（语法表——palette 命令「搜索语法帮助」唤起） */}
        {helpOpen && (
          <div
            role="dialog"
            aria-label="搜索语法帮助"
            style={{
              padding: tokens.spacing.md,
              borderTop: '1px solid rgba(55, 220, 242, 0.25)',
              fontSize: tokens.typography.caption.size + 1,
              color: tokens.color.textPrimary,
              lineHeight: 1.8,
            }}
          >
            <strong>高级搜索语法</strong>
            <div style={{ color: tokens.color.textSecondary, marginTop: 4 }}>
              path:文件夹 · tag:标签 · status:active|trashed|encrypted · type:note|folder|dailynote · date:2026-11|01..30|&gt;2026-10-01<br />
              "引号短语" 整词 · -词 排除（作用于标题+摘要） · 未知运算符自动按普通词并提示
            </div>
            <button onClick={() => setHelpOpen(false)} style={{ marginTop: 6, background: 'transparent', border: 'none', color: '#37DCF2', cursor: 'pointer' }}>
              关闭帮助
            </button>
          </div>
        )}
        <div
          style={{
            padding: tokens.spacing.sm,
            color: tokens.color.textDisabled,
            fontSize: tokens.typography.caption.size,
            borderTop: '1px solid rgba(255,255,255,0.08)',
          }}
        >
          ↑↓ 选择 · Enter 打开 · Esc 关闭 · 口语化可用（如「未完成的任务」）· 运算符 path:/tag:/status:/type:/date:
        </div>
      </div>
    </div>
  );
}

export default CommandPalette;
