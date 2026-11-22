/**
 * B-05（TA 2026-11-14 09:01 工单）: 编辑器往返一致性 property-based 测试。
 *
 * 随机块树 → docToMd → mdToNodes → 断言结构等价（docToMd/mdToDoc 同组
 * API 亦覆盖 A-03 源码模式 apply 链——工单「源码模式复用同一组测试」）。
 *
 * 随机源: mulberry32 种子 PRNG（BASE_SEED+用例号——失败可复现,报 seed）。
 * 用例量: ROUNDTRIP_CASES 环境变量（默认 1200 ≥ DoD 1000）。
 *
 * 生成域（工单明列六类全覆盖）:
 * paragraph（含 math_inline）/heading 1-6/code_block（含 mermaid 语言）/
 * blockquote/task_block/bullet+ordered 列表（含嵌套 case）/table（首行
 * table_header）/math_block/horizontal_rule/block_ref（A-05）。
 *
 * ── 差异白名单（显式声明——「允许的差异必须显式列白名单」工单原文）──
 * 以下差异为 v1 已知语义,comparator 归一后等价;白名单外差异=测试红:
 *  W1 嵌套列表拍平: docToMd inlineText 拍平 list_item 嵌套内容为文本
 *     （序列化无缩进语法——归一: 嵌套层文本按文档序并入首段）
 *  W2 task_id 丢弃: 序列化仅保 checked+文本（task_id=投影域键非内容）
 *  W3 code_block language 'plaintext'↔'' 归一（FENCE_RE 空语言解析为
 *     plaintext——语义等价）
 *  W4 容器三系（callout/super_block/tabs_block/embed）不进生成器:
 *     序列化-only（E24 既有测试覆盖 GFM alert/HTML 容器语序;解析侧
 *     v1 不识别——结构性不往返,不入随机域防白名单虚化）
 *  W5 空段落不生成: md 空行=块界语义（空段落无内容载体,落 md 即块界）
 *  W6 标题/任务/列表项内不嵌 math_inline: 解析侧 heading/task/listItem
 *     仅重建纯文本（math_inline 仅段落内往返）
 *  W7 table_cell colspan/rowspan 不生成: GFM 表格 v1 无 colspan 序列化
 *     语义（生成域即默认 1/1）
 *  W8 生成文本字符域受限: 排除 '$'(行内公式触发)/'|'(表格列界)/行首
 *     语法前缀（#/>/-/数字./((/```/$$/---）——往返域内文本字符收敛,
 *     非语法字符全量往返（受限字符为 md 语法保留字,属序列化器契约）
 *  W9 相邻 text 节点合并归一: 序列化拼接/解析单节点化后等价
 */
import { describe, it, expect } from 'vitest';
import { auroraSchema } from '../schema/auroraSchema';
import { docToMd, mdToDoc } from '../editors/auroraMarkdown';
import type { Node as ProseNode } from 'prosemirror-model';

/* ───────────────── 随机源（种子 PRNG——mulberry32） ───────────────── */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
type Rnd = () => number;
const pick = <T,>(r: Rnd, arr: readonly T[]): T => arr[Math.floor(r() * arr.length)];
const int = (r: Rnd, lo: number, hi: number): number => lo + Math.floor(r() * (hi - lo + 1));

/* W8: 安全词域——无 md 语法保留字 */
const WORDS = ['笔记', '内容', '文本', '块', 'Aurora', '测试', 'hello', '世界', '段落', '标题', 'a', 'b'] as const;
/* W6: 行内公式 latex 域——非空/无首尾空白/无 $（inlineNodes 契约） */
const LATEX_INLINE = ['E=mc^2', 'a+b', 'x_i', '\\alpha+\\beta', '\\frac{1}{2}', 'f(x)'] as const;
/* 块级 latex 域——无 $$ 行（多行 $$ 收集器契约） */
const LATEX_BLOCK = ['E=mc^2', '\\int_0^1 x dx', 'a_{n+1}=a_n+1'] as const;
/* W3: 语言域（含 mermaid——工单明列） */
const LANGS = ['rust', 'ts', 'mermaid', 'python', 'plaintext'] as const;

const N = auroraSchema.nodes;
const T = (t: string) => auroraSchema.text(t);
const P = (t: string) => N.paragraph.create(null, t ? T(t) : null);
const inlineChildren = (r: Rnd): ProseNode[] => {
  const out: ProseNode[] = [];
  const n = int(r, 1, 3);
  for (let i = 0; i < n; i++) {
    if (r() < 0.25) out.push(N.math_inline.create({ latex: pick(r, LATEX_INLINE) }));
    else out.push(T(pick(r, WORDS)));
  }
  return out;
};
const genText = (r: Rnd): string => {
  const n = int(r, 1, 3);
  const parts: string[] = [];
  for (let i = 0; i < n; i++) parts.push(pick(r, WORDS));
  return parts.join(' ');
};
const genNoteId = (r: Rnd): string => `n${int(r, 1000, 9999)}-${int(r, 10, 99)}`;

/** 单顶层块生成（W4/W5/W6/W7 约束内）。 */
function genBlock(r: Rnd, kind: number): ProseNode {
  switch (kind) {
    case 0:
      return N.paragraph.create(null, inlineChildren(r));
    case 1:
      return N.heading.create({ level: int(r, 1, 6) }, T(genText(r)));
    case 2: {
      const lang = pick(r, LANGS);
      const bodyN = int(r, 0, 3);
      const body: string[] = [];
      for (let i = 0; i < bodyN; i++) body.push(genText(r));
      return N.code_block.create(
        { language: lang },
        body.length ? [T(body.join('\n'))] : null,
      );
    }
    case 3: {
      const paras: ProseNode[] = [];
      const n = int(r, 1, 3);
      for (let i = 0; i < n; i++) paras.push(P(genText(r)));
      return N.blockquote.create(null, paras);
    }
    case 4:
      return N.task_block.create({ checked: r() < 0.5 }, T(genText(r)));
    case 5: {
      const items: ProseNode[] = [];
      const n = int(r, 1, 4);
      for (let i = 0; i < n; i++) items.push(N.list_item.create(null, [P(genText(r))]));
      return N.bullet_list.create(null, items);
    }
    case 6: {
      const items: ProseNode[] = [];
      const n = int(r, 1, 4);
      for (let i = 0; i < n; i++) items.push(N.list_item.create(null, [P(genText(r))]));
      return N.ordered_list.create(null, items);
    }
    case 7: {
      const rows: ProseNode[] = [];
      const rn = int(r, 2, 3);
      const cn = int(r, 1, 3);
      for (let i = 0; i < rn; i++) {
        const cells: ProseNode[] = [];
        for (let j = 0; j < cn; j++) cells.push(N.table_cell.create(null, [P(genText(r))]));
        rows.push((i === 0 ? N.table_header : N.table_row).create(null, cells));
      }
      return N.table.create(null, rows);
    }
    case 8:
      return N.math_block.create({ latex: pick(r, LATEX_BLOCK) });
    case 9:
      return N.horizontal_rule.create();
    case 10: {
      const anchor = r() < 0.4 ? genText(r).replace(/[()#]/g, '') : null;
      return N.block_ref.create({ noteId: genNoteId(r), anchor });
    }
    /* W1: 嵌套列表 case——bullet_list[li[para + 嵌套 bullet_list]] */
    case 11: {
      const inner: ProseNode[] = [];
      const n = int(r, 1, 2);
      for (let i = 0; i < n; i++) inner.push(N.list_item.create(null, [P(genText(r))]));
      const outer = N.list_item.create(null, [P(genText(r)), N.bullet_list.create(null, inner)]);
      return N.bullet_list.create(null, [outer]);
    }
    default:
      return P(genText(r));
  }
}

/* ───────────────── 归一比较器（白名单 W1/W2/W3/W9 落地处） ───────────────── */
interface Canon {
  t: string;
  a?: Record<string, unknown>;
  /* 行内序列: ['文本'|'公式', 值] 交替（W9 相邻合并） */
  inline?: Array<[string, string]>;
  c?: Canon[];
}

function canonInline(nodes: readonly ProseNode[]): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  for (const n of nodes) {
    if (n.type.name === 'math_inline') out.push(['公式', String(n.attrs.latex)]);
    else {
      const t = n.textContent;
      if (out.length && out[out.length - 1][0] === '文本') out[out.length - 1][1] += t; /* W9 */
      else out.push(['文本', t]);
    }
  }
  return out;
}

function canon(node: ProseNode): Canon {
  const name = node.type.name;
  switch (name) {
    case 'paragraph':
      return { t: 'p', inline: canonInline(node.content ? node.content.content : []) };
    case 'heading':
      return { t: 'h', a: { level: node.attrs.level }, inline: canonInline(node.content ? node.content.content : []) };
    case 'code_block':
      return { t: 'code', a: { language: node.attrs.language || 'plaintext' }, inline: [['文本', node.textContent]] }; /* W3 */
    case 'blockquote':
      return { t: 'quote', c: node.content!.content.map(canon) };
    case 'task_block':
      return { t: 'task', a: { checked: node.attrs.checked === true }, inline: [['文本', node.textContent]] }; /* W2: task_id 弃 */
    case 'bullet_list':
    case 'ordered_list':
      return { t: name === 'bullet_list' ? 'ul' : 'ol', c: node.content!.content.map(canon) };
    case 'list_item':
      return { t: 'li', c: node.content ? node.content.content.map(canon) : [] };
    case 'table':
      return { t: 'table', c: node.content!.content.map(canon) };
    case 'table_header':
    case 'table_row':
      return { t: name === 'table_header' ? 'th' : 'tr', c: node.content!.content.map(canon) };
    case 'table_cell':
      return { t: 'td', c: node.content ? node.content.content.map(canon) : [] }; /* W7 */
    case 'math_block':
      return { t: '$$', a: { latex: String(node.attrs.latex) } };
    case 'horizontal_rule':
      return { t: 'hr' };
    case 'block_ref':
      return { t: 'ref', a: { noteId: node.attrs.noteId, anchor: node.attrs.anchor ?? null } };
    default:
      return { t: name };
  }
}

/** W1: 源侧归一——list_item 嵌套内容（首段外的块）拍平并入首段文本（文档序）。 */
function flattenListItems(node: ProseNode): ProseNode {
  if (node.type.name !== 'list_item' || !node.content) return node;
  const kids = node.content.content;
  const first = kids[0];
  const rest = kids.slice(1);
  if (rest.length === 0) return node;
  /* 嵌套文本按文档序并入首段（docToMd inlineText 拍平语义） */
  const merged = `${first.textContent}${rest.map((x) => x.textContent).join('')}`;
  const inline = first.content
    ? first.content.content.map((c: ProseNode) =>
        c.type.name === 'math_inline'
          ? c
          : auroraSchema.text(`${c.textContent}${rest.map((x) => x.textContent).join('')}`),
      )
    : null;
  /* 首段无 math_inline 时简化为纯文本段（生成器 list 项为纯文本——W6） */
  const li = N.list_item.create(null, [P(merged)]);
  return inline && first.content!.content.some((c) => c.type.name === 'math_inline') ? N.list_item.create(null, [N.paragraph.create(null, inline)]) : li;
}

function canonDocTop(blocks: readonly ProseNode[], normalizeLists: boolean): Canon[] {
  return blocks.map((b) =>
    normalizeLists && (b.type.name === 'bullet_list' || b.type.name === 'ordered_list')
      ? canon(b.type.create(b.attrs, b.content!.content.map(flattenListItems)))
      : canon(b),
  );
}

/* ───────────────── 用例 ───────────────── */
const CASES = parseInt(process.env.ROUNDTRIP_CASES ?? '1200', 10);
const BASE_SEED = 0xb05_2026_1114;

describe('B-05 往返一致性 property 测试', () => {
  it(`随机块树 docToMd→mdToNodes 结构等价（${CASES} 用例·白名单 W1-W9 外零差异）`, () => {
    let checked = 0;
    for (let i = 0; i < CASES; i++) {
      const seed = BASE_SEED + i;
      const r = mulberry32(seed);
      const blockCount = int(r, 1, 6);
      const kinds: number[] = [];
      for (let b = 0; b < blockCount; b++) kinds.push(int(r, 0, 11));
      /* 同型列表相邻合并检查: 生成器产独立顶层块,docToMd 块间空行保界 */
      const blocks = kinds.map((k) => genBlock(r, k));
      const doc = N.doc.create(null, blocks);

      const md = docToMd(doc);
      const back = mdToDoc(auroraSchema, md);

      const left = canonDocTop(blocks, true); /* 源侧归一（W1） */
      const right = canonDocTop(back.content!.content, false);
      try {
        expect(right).toEqual(left);
      } catch {
        throw new Error(
          `往返差异（seed=${seed} case=${i} kinds=[${kinds.join(',')}]）\n` +
            `md 产物:\n${md}\n` +
            `源(归一): ${JSON.stringify(left)}\n` +
            `回: ${JSON.stringify(right)}`,
        );
      }
      checked++;
    }
    expect(checked).toBe(CASES);
  });

  it('种子可复现性——同 seed 生成同树', () => {
    const gen = (seed: number): string => {
      const r = mulberry32(seed);
      const blocks = [genBlock(r, 5)];
      return docToMd(N.doc.create(null, blocks));
    };
    expect(gen(BASE_SEED + 7)).toBe(gen(BASE_SEED + 7));
  });

  it('md→doc→md 幂等（二次序列化稳定——落库重读不漂移）', () => {
    const r = mulberry32(BASE_SEED + 99991);
    const blocks = [genBlock(r, 7), genBlock(r, 1), genBlock(r, 5)];
    const md1 = docToMd(N.doc.create(null, blocks));
    const md2 = docToMd(mdToDoc(auroraSchema, md1));
    expect(md2).toBe(md1);
  });
});
