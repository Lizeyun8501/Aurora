// V19 §35.1 Block 类型定义 — Aurora ProseMirror Schema（DEV-009）
// 节点: doc/paragraph/heading/code_block/task_block/embed/blockquote/lists/hr
// 标记: strong/em/underline/strikethrough/code/link

import { Schema } from 'prosemirror-model';
import OrderedMap from 'orderedmap';
import { addListNodes } from 'prosemirror-schema-list';

const baseNodes = {
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
    attrs: { level: { default: 1, validate: 'number' } },
    toDOM: (n) => [`h${n.attrs.level}`, 0],
    parseDOM: [1, 2, 3, 4, 5, 6].map((l) => ({
      tag: `h${l}`,
      attrs: { level: l },
    })),
  },

  code_block: {
    group: 'block',
    content: 'text*',
    marks: '',
    code: true,
    attrs: { language: { default: 'plaintext' } },
    toDOM: (n) => ['pre', { 'data-language': n.attrs.language }, ['code', 0]],
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
    attrs: { checked: { default: false, validate: 'boolean' }, task_id: { default: null } },
    toDOM: (n) => [
      'div',
      {
        class: 'task-block',
        'data-checked': String(n.attrs.checked),
        'data-task-id': n.attrs.task_id || '',
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
    toDOM: (n) => [
      'div',
      {
        class: 'embed-block',
        'data-embed-type': n.attrs.embed_type,
        'data-url': n.attrs.url,
      },
      n.attrs.title || n.attrs.url,
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

  // 引用块
  blockquote: {
    group: 'block',
    content: 'block+',
    toDOM: () => ['blockquote', 0],
    parseDOM: [{ tag: 'blockquote' }],
  },

  // 分割线
  horizontal_rule: {
    group: 'block',
    toDOM: () => ['hr'],
    parseDOM: [{ tag: 'hr' }],
  },

  text: { group: 'inline', inline: true },
};

const baseMarks = {
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
    toDOM: (n) => ['a', { href: n.attrs.href, title: n.attrs.title || undefined }, 0],
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
};

// addListNodes: 注入 bullet_list/ordered_list/list_item（要求 OrderedMap）
export const auroraSchema = new Schema({
  nodes: addListNodes(OrderedMap.from(baseNodes), 'block+', 'block'),
  marks: baseMarks,
});
