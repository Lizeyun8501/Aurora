/**
 * Aurora markdown-ish 往返 — DK-05 S2 桌面编辑态落库序列化。
 *
 * 语义对齐既有 content 形态（mock 数据 `# 标题` / `- 列表` 行）：
 * 块级 = heading(1-3) / code_block(fence) / blockquote / task_block /
 *       bullet_list / ordered_list / horizontal_rule / paragraph；
 * 内联标记 S2 保持原样文本（往返一致优先；富内联解析 S3+ 扩展）。
 *
 * S2 落库链路：PM doc → docToMd → cmd_update_note(content)（禁绕过）；
 * 读取链路：content → mdToDoc → 初始 doc（存→取→渲染等价验收）。
 */
import type { Node as ProseNode, Schema } from 'prosemirror-model';

/** 代码围栏正则（```lang ... ```） */
const FENCE_RE = /^```(\w*)\s*$/;
/** 任务行（- [x] / - [ ]） */
const TASK_RE = /^- \[( |x)\] (.*)$/;
/** 无序行（- / *） */
const BULLET_RE = /^[-*] (.*)$/;
/** 有序行（1. / 1)） */
const ORDERED_RE = /^\d+[.)] (.*)$/;

type AnySchema = Schema<string, any>;

/**
 * markdown-ish 文本 → ProseMirror doc 节点数组。
 * 相邻同类列表行合并进同一 list 节点（list_item 内 paragraph）。
 */
export function mdToNodes(schema: AnySchema, text: string): ProseNode[] {
  const lines = text.split('\n');
  const nodes: ProseNode[] = [];
  let i = 0;
  const para = (t: string) =>
    schema.nodes.paragraph.create(null, t ? [schema.text(t)] : null);
  const listItem = (t: string) =>
    schema.nodes.list_item.create(null, para(t));

  while (i < lines.length) {
    const line = lines[i];

    // 空行跳过
    if (!line.trim()) { i++; continue; }

    // 代码围栏
    const fence = FENCE_RE.exec(line);
    if (fence) {
      const lang = fence[1] || 'plaintext';
      const body: string[] = [];
      i++;
      while (i < lines.length && !FENCE_RE.test(lines[i])) { body.push(lines[i]); i++; }
      i++; // 跳过闭合 fence（无闭合则到尾）
      nodes.push(
        schema.nodes.code_block.create(
          { language: lang },
          body.length ? [schema.text(body.join('\n'))] : null,
        ),
      );
      continue;
    }

    // 标题
    const heading = /^(#{1,3}) (.*)$/.exec(line);
    if (heading) {
      nodes.push(
        schema.nodes.heading.create(
          { level: heading[1].length },
          heading[2] ? [schema.text(heading[2])] : null,
        ),
      );
      i++;
      continue;
    }

    // 引用（连续 > 行合并为单 blockquote 内多段落）
    if (line.startsWith('> ')) {
      const body: ProseNode[] = [];
      while (i < lines.length && lines[i].startsWith('> ')) {
        body.push(para(lines[i].slice(2)));
        i++;
      }
      nodes.push(schema.nodes.blockquote.create(null, body));
      continue;
    }

    // 任务块
    const task = TASK_RE.exec(line);
    if (task) {
      nodes.push(
        schema.nodes.task_block.create(
          { checked: task[1] === 'x' },
          task[2] ? [schema.text(task[2])] : null,
        ),
      );
      i++;
      continue;
    }

    // 无序列表（相邻行合并）
    if (BULLET_RE.test(line)) {
      const items: ProseNode[] = [];
      while (i < lines.length && BULLET_RE.test(lines[i])) {
        items.push(listItem(BULLET_RE.exec(lines[i])![1]));
        i++;
      }
      nodes.push(schema.nodes.bullet_list.create(null, items));
      continue;
    }

    // 有序列表（相邻行合并）
    if (ORDERED_RE.test(line)) {
      const items: ProseNode[] = [];
      while (i < lines.length && ORDERED_RE.test(lines[i])) {
        items.push(listItem(ORDERED_RE.exec(lines[i])![1]));
        i++;
      }
      nodes.push(schema.nodes.ordered_list.create(null, items));
      continue;
    }

    // DK-2.7 切片二：GFM 表格（| a | b | 行 + |---|---| 分隔行确认——
    // 连续 | 行整块收集；cell 纯文本段落（inline 富文本 v1 简化——诚实边界）
    if (/^\|.+\|\s*$/.test(line.trim()) && i + 1 < lines.length && /^\|[\s:|-]+\|\s*$/.test(lines[i + 1].trim())) {
      const rows: string[][] = [];
      const cellsOf = (l: string) =>
        l
          .trim()
          .replace(/^\|/, '')
          .replace(/\|$/, '')
          .split('|')
          .map((c) => c.trim());
      // v1 约定：header 行作为数据首行保留（schema 无 table_header 节点——
      // 表头样式区分延后）；分隔行仅作 GFM 确认不进表
      rows.push(cellsOf(line));
      i += 2; // 跳过 header+分隔行
      while (i < lines.length && /^\|.+\|\s*$/.test(lines[i].trim())) {
        rows.push(cellsOf(lines[i]));
        i++;
      }
      if (rows.length) {
        const cell = (t: string) =>
          schema.nodes.table_cell.create(null, [para(t)]);
        const mkRow = (cells: string[], header: boolean) => {
          const NodeType = header ? schema.nodes.table_header : schema.nodes.table_row;
          return NodeType.create(null, cells.map(cell));
        };
        // 切片四：首行=table_header（th 语义），余行 table_row
        nodes.push(
          schema.nodes.table.create(null, [
            mkRow(rows[0], true),
            ...rows.slice(1).map((r) => mkRow(r, false)),
          ]),
        );
      }
      continue;
    }

    // 水平线
    if (/^-{3,}$/.test(line.trim())) { nodes.push(schema.nodes.horizontal_rule.create()); i++; continue; }

    // 段落
    nodes.push(para(line));
    i++;
  }
  return nodes;
}

/** markdown-ish 文本 → 完整 doc 节点（供 replaceWith 初始装载） */
export function mdToDoc(schema: AnySchema, text: string): ProseNode {
  return schema.nodes.doc.create(null, mdToNodes(schema, text));
}

/** 单块节点 → markdown 行（docToMd 递归用） */
function nodeToMd(node: ProseNode, orderedIdx: { n: number }): string[] {
  const t = node.textContent;
  switch (node.type.name) {
    case 'heading': return [`${'#'.repeat(node.attrs.level)} ${t}`.trimEnd()];
    case 'code_block': {
      const lang = node.attrs.language && node.attrs.language !== 'plaintext' ? node.attrs.language : '';
      return [`\`\`\`${lang}`, t, '```'];
    }
    case 'blockquote': {
      const inner = node.content.content.flatMap((c: ProseNode) => nodeToMd(c, orderedIdx));
      return inner.map((l) => `> ${l}`);
    }
    case 'task_block': return [`- [${node.attrs.checked ? 'x' : ' '}] ${t}`.trimEnd()];
    case 'bullet_list':
      return node.content.content.flatMap((li: ProseNode) => [`- ${li.textContent}`]);
    case 'ordered_list':
      return node.content.content.flatMap((li: ProseNode) => [`${orderedIdx.n++}. ${li.textContent}`]);
    case 'horizontal_rule': return ['---'];
    case 'paragraph': return [t];
    // S2 工具条外类型：占位行保底（内容不丢，往返可再扩）
    case 'embed': return [`[embed:${node.attrs.url || node.attrs.embed_type}]`];
    case 'ai_suggestion': return [`[ai:${node.attrs.suggestionType}] ${t}`];
    case 'table': {
      // DK-2.7 切片二：GFM 序列化——首行+分隔行补齐（schema 无 table_header
      // 节点——首行按普通行输出+分隔行使 GFM 合法）；cell 取 textContent
      const rows = node.content.content as ProseNode[];
      const cells = (r: ProseNode) =>
        (r.content.content as ProseNode[]).map((c) => c.textContent.trim());
      const lines: string[] = [];
      rows.forEach((r, idx) => {
        const row = cells(r);
        lines.push(`| ${row.join(' | ')} |`);
        if (idx === 0) {
          lines.push(`| ${row.map(() => '---').join(' | ')} |`);
        }
      });
      return lines;
    }
    default: return [t];
  }
}

/** ProseMirror doc → markdown-ish 文本（落库序列化） */
export function docToMd(doc: ProseNode): string {
  const idx = { n: 1 };
  const lines: string[] = [];
  doc.content.content.forEach((node: ProseNode) => {
    idx.n = 1; // 每个顶层有序列表重新计数
    lines.push(...nodeToMd(node, idx));
  });
  return lines.join('\n');
}
