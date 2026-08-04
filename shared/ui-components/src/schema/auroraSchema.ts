/**
 * Aurora ProseMirror Schema — V19 §35
 *
 * 定义块级文档的自定义 Schema，包括标准节点（paragraph, heading, code,
 * blockquote, table, list_item, divider）和 Aurora 自定义块（task_block,
 * embed, ai_suggestion）。
 *
 * 与 TipTap 的集成方式：
 * - TipTap StarterKit 提供标准节点（paragraph, heading, code 等）
 * - Aurora 自定义块通过 TipTap Node Extension 注册
 * - 本文件导出纯 Schema 定义（`auroraSchema`）和 TipTap Extensions 两种形式
 */

import { Schema } from '@tiptap/pm/model';
import type { Node as ProseNode, Mark } from '@tiptap/pm/model';

// ── Aurora 自定义属性类型 ─────────────────────────────────────

export interface TaskBlockAttrs {
  /** 关联的 GTD 任务 ID */
  taskId: string | null;
  /** 任务状态：inbox | next | scheduled | doing | done */
  status: 'inbox' | 'next' | 'scheduled' | 'doing' | 'done';
  /** 优先级：0-4 */
  priority: number;
  /** 截止日期（ISO 8601） */
  dueDate: string | null;
}

export interface EmbedAttrs {
  /** 嵌入资源 URL */
  src: string;
  /** 嵌入类型：note | link | video | audio | file */
  type: 'note' | 'link' | 'video' | 'audio' | 'file';
  /** 显示标题 */
  title?: string;
}

export interface AISuggestionAttrs {
  /** 建议类型：grammar | rewrite | continue | summarize */
  suggestionType: 'grammar' | 'rewrite' | 'continue' | 'summarize';
  /** 建议来源模型 */
  model: string;
  /** 是否已接受 */
  accepted: boolean;
}

// ── ProseMirror Schema 定义 ───────────────────────────────────

/**
 * Aurora 自定义 ProseMirror Schema。
 *
 * 对应 V19 §35.1 Block 类型定义。
 * 标准节点（doc, paragraph, heading, code, blockquote, table, list_item,
 * divider, text）+ 自定义块（task_block, embed, ai_suggestion）。
 *
 * 标记（marks）：bold, italic, code, link, highlight。
 */
export const auroraSchema = new Schema({
  nodes: {
    // ── 文档根节点 ──
    doc: { content: 'block+' },

    // ── 标准块节点 ──
    paragraph: {
      group: 'block',
      content: 'inline*',
      toDOM: () => ['p', 0],
      parseDOM: [{ tag: 'p' }],
    },

    heading: {
      group: 'block',
      content: 'inline*',
      attrs: { level: { default: 1, validate: 'integer' } },
      toDOM: (node: ProseNode) => [`h${node.attrs.level}`, 0],
      parseDOM: [1, 2, 3, 4, 5, 6].map((level) => ({
        tag: `h${level}`,
        getAttrs: () => ({ level }),
      })),
    },

    code: {
      group: 'block',
      content: 'text*',
      code: true,
      toDOM: () => ['pre', ['code', 0]],
      parseDOM: [{ tag: 'pre' }],
    },

    blockquote: {
      group: 'block',
      content: 'block+',
      toDOM: () => ['blockquote', 0],
      parseDOM: [{ tag: 'blockquote' }],
    },

    // ── 表格节点 ──
    table: {
      group: 'block',
      content: 'table_row+',
      toDOM: () => ['table', ['tbody', 0]],
      parseDOM: [{ tag: 'table' }],
    },

    table_row: {
      content: 'table_cell+',
      toDOM: () => ['tr', 0],
      parseDOM: [{ tag: 'tr' }],
    },

    table_cell: {
      content: 'block+',
      attrs: {
        colspan: { default: 1 },
        rowspan: { default: 1 },
      },
      toDOM: (node: ProseNode) => [
        'td',
        { colspan: node.attrs.colspan, rowspan: node.attrs.rowspan },
        0,
      ],
      parseDOM: [{ tag: 'td' }],
    },

    // ── 列表与分隔 ──
    list_item: {
      content: 'paragraph block*',
      toDOM: () => ['li', 0],
      parseDOM: [{ tag: 'li' }],
    },

    divider: {
      group: 'block',
      toDOM: () => ['hr'],
      parseDOM: [{ tag: 'hr' }],
    },

    // ── Aurora 自定义块 ──

    /** GTD 任务块 — 绑定 GTD 任务系统 */
    task_block: {
      group: 'block',
      content: 'paragraph',
      attrs: {
        taskId: { default: null },
        status: { default: 'inbox' },
        priority: { default: 0 },
        dueDate: { default: null },
      },
      toDOM: (node: ProseNode) => [
        'div',
        {
          class: 'task-block',
          'data-task-id': node.attrs.taskId,
          'data-status': node.attrs.status,
          'data-priority': node.attrs.priority,
          'data-due-date': node.attrs.dueDate,
        },
        0,
      ],
      parseDOM: [
        {
          tag: 'div.task-block',
          getAttrs: (dom: HTMLElement) => ({
            taskId: dom.getAttribute('data-task-id'),
            status: dom.getAttribute('data-status') || 'inbox',
            priority: parseInt(dom.getAttribute('data-priority') || '0', 10),
            dueDate: dom.getAttribute('data-due-date'),
          }),
        },
      ],
    },

    /** 嵌入块 — 嵌入其他笔记、链接、视频等 */
    embed: {
      group: 'block',
      atom: true,
      attrs: {
        src: {},
        type: { default: 'note' },
        title: { default: null },
      },
      toDOM: (node: ProseNode) => [
        'div',
        {
          class: 'embed',
          'data-src': node.attrs.src,
          'data-type': node.attrs.type,
          'data-title': node.attrs.title,
        },
      ],
      parseDOM: [
        {
          tag: 'div.embed',
          getAttrs: (dom: HTMLElement) => ({
            src: dom.getAttribute('data-src') || '',
            type: dom.getAttribute('data-type') || 'note',
            title: dom.getAttribute('data-title'),
          }),
        },
      ],
    },

    /** AI 建议块 — AI 生成的建议内容，用户可接受或拒绝 */
    ai_suggestion: {
      group: 'block',
      content: 'inline*',
      attrs: {
        suggestionType: { default: 'continue' },
        model: { default: 'unknown' },
        accepted: { default: false },
      },
      toDOM: (node: ProseNode) => [
        'div',
        {
          class: 'ai-suggestion',
          'data-suggestion-type': node.attrs.suggestionType,
          'data-model': node.attrs.model,
          'data-accepted': node.attrs.accepted,
        },
        0,
      ],
      parseDOM: [
        {
          tag: 'div.ai-suggestion',
          getAttrs: (dom: HTMLElement) => ({
            suggestionType: dom.getAttribute('data-suggestion-type') || 'continue',
            model: dom.getAttribute('data-model') || 'unknown',
            accepted: dom.getAttribute('data-accepted') === 'true',
          }),
        },
      ],
    },

    // ── 文本节点 ──
    text: {
      inline: true,
      group: 'inline',
    },
  },

  marks: {
    bold: {
      toDOM: () => ['strong', 0],
      parseDOM: [{ tag: 'strong' }, { tag: 'b' }],
    },

    italic: {
      toDOM: () => ['em', 0],
      parseDOM: [{ tag: 'em' }, { tag: 'i' }],
    },

    code: {
      toDOM: () => ['code', 0],
      parseDOM: [{ tag: 'code' }],
    },

    link: {
      attrs: {
        href: {},
        title: { default: null },
      },
      inclusive: false,
      toDOM: (node: Mark) => [
        'a',
        { href: node.attrs.href, title: node.attrs.title },
        0,
      ],
      parseDOM: [
        {
          tag: 'a[href]',
          getAttrs: (dom: HTMLElement) => ({
            href: dom.getAttribute('href'),
            title: dom.getAttribute('title'),
          }),
        },
      ],
    },

    /** 高亮标记 — 支持自定义颜色 */
    highlight: {
      attrs: { color: { default: '#fef08a' } },
      toDOM: (node: Mark) => [
        'mark',
        { style: `background-color: ${node.attrs.color}` },
        0,
      ],
      parseDOM: [
        {
          tag: 'mark',
          getAttrs: (dom: HTMLElement) => ({
            color: dom.style.backgroundColor || '#fef08a',
          }),
        },
      ],
    },
  },
});

// ── 块类型常量（供 TipTap Extension 注册使用） ─────────────

export const AURORA_BLOCK_TYPES = {
  PARAGRAPH: 'paragraph',
  HEADING: 'heading',
  CODE: 'code',
  BLOCKQUOTE: 'blockquote',
  TABLE: 'table',
  TABLE_ROW: 'table_row',
  TABLE_CELL: 'table_cell',
  LIST_ITEM: 'list_item',
  DIVIDER: 'divider',
  TASK_BLOCK: 'task_block',
  EMBED: 'embed',
  AI_SUGGESTION: 'ai_suggestion',
} as const;

export type AuroraBlockType =
  (typeof AURORA_BLOCK_TYPES)[keyof typeof AURORA_BLOCK_TYPES];

// ── CSS 样式常量 ──────────────────────────────────────────────

export const AURORA_BLOCK_CSS = {
  taskBlock: 'task-block',
  embed: 'embed',
  aiSuggestion: 'ai-suggestion',
  taskStatus: {
    inbox: 'task-inbox',
    next: 'task-next',
    scheduled: 'task-scheduled',
    doing: 'task-doing',
    done: 'task-done',
  },
} as const;
