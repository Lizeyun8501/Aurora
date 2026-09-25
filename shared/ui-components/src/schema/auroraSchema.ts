/**
 * Aurora ProseMirror Schema — V19 §35 · DK-0F 合并版
 *
 * 2026-09-25（editor-uplift 卡）：共享层设计版（334 行，TipTap re-export，
 * 双端零使用）与 mobile 实战版（168 行，真机编辑器绑定）合并。
 * 合并基线：**mobile 实战语义优先**（RichEditor/auroraEditor 的 loro 绑定
 * 硬约束），共享层增量（table 系 / ai_suggestion / highlight mark / 类型
 * 与常量导出面）原样并入。
 *
 * 冲突裁决（与 RichEditor toolbar / dk05mv 数据链一致）：
 * - code_block（带 language）取代设计版 code
 * - task_block attrs = { checked, task_id }（GTD 双向绑定 tasks 容器）
 * - embed attrs = { embed_type, url, title }（draggable）
 * - marks 用 strong/em 命名 + underline/strikethrough 全量
 * - 列表经 addListNodes 注入（bullet_list/ordered_list/list_item）
 * - horizontal_rule 取代 divider（AURORA_BLOCK_TYPES.DIVIDER 保留键名）
 *
 * import 为原生 prosemirror-model —— TipTap 已随 DK-0F 移除（ADR-005 D2）。
 */

import { Schema, type NodeSpec, type MarkSpec, type Node as ProseNode, type Mark } from 'prosemirror-model';
import OrderedMap from 'orderedmap';
import { addListNodes } from 'prosemirror-schema-list';

// ── Aurora 自定义属性类型 ─────────────────────────────────────

export interface TaskBlockAttrs {
  /** 勾选状态（与 GTD tasks 容器双向绑定） */
  checked: boolean;
  /** 关联的 GTD 任务 ID */
  task_id: string | null;
}

export interface EmbedAttrs {
  /** 嵌入类型：link | image | file | map | web */
  embed_type: 'link' | 'image' | 'file' | 'map' | 'web';
  /** 嵌入资源 URL */
  url: string;
  /** 显示标题 */
  title: string;
}

export interface AISuggestionAttrs {
  /** 建议类型：grammar | rewrite | continue | summarize */
  suggestionType: 'grammar' | 'rewrite' | 'continue' | 'summarize';
  /** 建议来源模型 */
  model: string;
  /** 是否已接受 */
  accepted: boolean;
}

// ── 节点定义（mobile 实战版 + 共享层增量） ─────────────────────

const baseNodes: Record<string, NodeSpec> = {
  doc: { content: 'block+' },

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

  // 代码块（带语言标注；取代设计版 code —— RichEditor toolbar 绑定）
  code_block: {
    group: 'block',
    content: 'text*',
    marks: '',
    code: true,
    attrs: { language: { default: 'plaintext' } },
    toDOM: (node: ProseNode) => [
      'pre',
      { 'data-language': node.attrs.language },
      ['code', 0],
    ],
    parseDOM: [
      {
        tag: 'pre',
        getAttrs: (dom: HTMLElement) => ({
          language: dom.getAttribute('data-language') || 'plaintext',
        }),
      },
    ],
  },

  // V19 §35.1 任务块 — GTD 集成（勾选状态双向绑定 tasks 容器）
  task_block: {
    group: 'block',
    content: 'inline*',
    attrs: {
      checked: { default: false, validate: 'boolean' },
      task_id: { default: null },
    },
    toDOM: (node: ProseNode) => [
      'div',
      {
        class: 'task-block',
        'data-checked': String(node.attrs.checked),
        'data-task-id': node.attrs.task_id || '',
      },
      0,
    ],
    parseDOM: [
      {
        tag: 'div.task-block',
        getAttrs: (dom: HTMLElement) => ({
          checked: dom.getAttribute('data-checked') === 'true',
          task_id: dom.getAttribute('data-task-id') || null,
        }),
      },
    ],
  },

  // V19 §35.1 嵌入块 — 外部资源（图片/附件/地图/网页卡片）
  embed: {
    group: 'block',
    attrs: {
      embed_type: { default: 'link' }, // link | image | file | map | web
      url: { default: '' },
      title: { default: '' },
    },
    draggable: true,
    toDOM: (node: ProseNode) => [
      'div',
      {
        class: 'embed-block',
        'data-embed-type': node.attrs.embed_type,
        'data-url': node.attrs.url,
      },
      node.attrs.title || node.attrs.url,
    ],
    parseDOM: [
      {
        tag: 'div.embed-block',
        getAttrs: (dom: HTMLElement) => ({
          embed_type: dom.getAttribute('data-embed-type') || 'link',
          url: dom.getAttribute('data-url') || '',
          title: dom.textContent || '',
        }),
      },
    ],
  },

  blockquote: {
    group: 'block',
    content: 'block+',
    toDOM: () => ['blockquote', 0],
    parseDOM: [{ tag: 'blockquote' }],
  },

  // ── 表格节点（共享层增量，DK-05 后续切片接入编辑 UI） ──
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

  // 分割线（实战命名 horizontal_rule；设计版 divider 键名保留于常量表）
  horizontal_rule: {
    group: 'block',
    toDOM: () => ['hr'],
    parseDOM: [{ tag: 'hr' }],
  },

  // AI 建议块（共享层增量 — DK-10 AI 网关钩子）
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

  text: { group: 'inline', inline: true },
};

const baseMarks: Record<string, MarkSpec> = {
  strong: {
    toDOM: () => ['strong', 0],
    parseDOM: [
      { tag: 'strong' },
      { tag: 'b' },
      {
        style: 'font-weight',
        getAttrs: (v: string) => (/^(bold(er)?|[5-9]\d{2,})$/.test(v) ? null : false),
      },
    ],
  },
  em: {
    toDOM: () => ['em', 0],
    parseDOM: [{ tag: 'em' }, { tag: 'i' }, { style: 'font-style=italic' }],
  },
  underline: {
    toDOM: () => ['u', 0],
    parseDOM: [{ tag: 'u' }, { style: 'text-decoration=underline' }],
  },
  strikethrough: {
    toDOM: () => ['del', 0],
    parseDOM: [{ tag: 'del' }, { tag: 's' }, { tag: 'strike' }],
  },
  code: {
    toDOM: () => ['code', 0],
    parseDOM: [{ tag: 'code' }],
  },
  link: {
    attrs: { href: { default: '' }, title: { default: null } },
    inclusive: false,
    toDOM: (mark: Mark) => [
      'a',
      { href: mark.attrs.href, title: mark.attrs.title || undefined },
      0,
    ],
    parseDOM: [
      {
        tag: 'a[href]',
        getAttrs: (dom: HTMLElement) => ({
          href: dom.getAttribute('href') || '',
          title: dom.getAttribute('title'),
        }),
      },
    ],
  },
  /** 高亮标记（共享层增量）— 支持自定义颜色 */
  highlight: {
    attrs: { color: { default: '#fef08a' } },
    toDOM: (mark: Mark) => [
      'mark',
      { style: `background-color: ${mark.attrs.color}` },
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
};

// addListNodes: 注入 bullet_list/ordered_list/list_item（要求 OrderedMap）
export const auroraSchema = new Schema({
  nodes: addListNodes(OrderedMap.from(baseNodes), 'block+', 'block'),
  marks: baseMarks,
});

// ── 块类型常量（供 UI/测试/DK-05 编辑器接入使用） ─────────────

export const AURORA_BLOCK_TYPES = {
  PARAGRAPH: 'paragraph',
  HEADING: 'heading',
  CODE_BLOCK: 'code_block',
  BLOCKQUOTE: 'blockquote',
  TABLE: 'table',
  TABLE_ROW: 'table_row',
  TABLE_CELL: 'table_cell',
  LIST_ITEM: 'list_item',
  BULLET_LIST: 'bullet_list',
  ORDERED_LIST: 'ordered_list',
  HORIZONTAL_RULE: 'horizontal_rule',
  /** @deprecated 旧设计版键名（= horizontal_rule），兼容保留 */
  DIVIDER: 'horizontal_rule',
  TASK_BLOCK: 'task_block',
  EMBED: 'embed',
  AI_SUGGESTION: 'ai_suggestion',
} as const;

export type AuroraBlockType =
  (typeof AURORA_BLOCK_TYPES)[keyof typeof AURORA_BLOCK_TYPES];

// ── CSS 样式常量 ──────────────────────────────────────────────

export const AURORA_BLOCK_CSS = {
  taskBlock: 'task-block',
  embed: 'embed-block',
  aiSuggestion: 'ai-suggestion',
} as const;
