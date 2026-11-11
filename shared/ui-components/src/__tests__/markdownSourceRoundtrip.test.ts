import { describe, it, expect } from 'vitest';
import type { Node as ProseNode } from 'prosemirror-model';
import { auroraSchema } from '../schema/auroraSchema';
import { mdToNodes, docToMd } from '../editors/auroraMarkdown';

/**
 * A-03 property-based 往返测试（TA 工单:随机块树 → md → 块树,断言结构等价
 * 或列出允许的差异）。
 *
 * 生成器: mulberry32 种子 PRNG（可复现——失败即 seed 报告）×100 随机块树。
 * 覆盖 DoD 块型: heading(1-6) / paragraph(含行内公式) / task_block / code_block
 * (含 mermaid) / blockquote / bullet_list / ordered_list / horizontal_rule /
 * table(header+rows) / math_block。
 *
 * 允许差异（allowlist——语义等价,非丢失）:
 * - 相邻同型列表在 md 面天然合并（发射器逐 list 独立、解析器按行合并）——
 *   生成器规避: 永不生成相邻同型列表,断言零 allowlist 差异（严格面）。
 * - 空段落被解析器跳过——生成器不生成空文本段落。
 */

/** mulberry32 — 种子 PRNG（32bit,可复现）。 */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

type Rng = () => number;
const pick = <T,>(r: Rng, xs: readonly T[]): T => xs[Math.floor(r() * xs.length)];
const intBetween = (r: Rng, lo: number, hi: number): number =>
  lo + Math.floor(r() * (hi - lo + 1));

/** 无冲突词表（避 |、$、`、#、>、-、* 等语法字符——纯内容面）。 */
const WORDS = ['aurora', '笔记', 'note', '测试', 'abc', 'xyz', '数据', '块', 'text'];
const word = (r: Rng): string => pick(r, WORDS);
const sentence = (r: Rng, n = intBetween(r, 1, 4)): string =>
  Array.from({ length: n }, () => word(r)).join(' ');

const LATEX = ['x^2+1', 'a+b', 'f(x)', 'E=mc^2', '\\pi r^2'];

/** 生成一个顶层块（ProseNode）。 */
function genBlock(r: Rng): ProseNode {
  const sch = auroraSchema;
  const kind = pick(r, [
    'heading', 'paragraph', 'paragraphMath', 'task', 'code', 'quote',
    'bullet', 'ordered', 'hr', 'table', 'mathblock',
  ] as const);
  switch (kind) {
    case 'heading': {
      const level = intBetween(r, 1, 6);
      return sch.nodes.heading.create({ level }, sch.text(sentence(r)));
    }
    case 'paragraph':
      return sch.nodes.paragraph.create(null, sch.text(sentence(r)));
    case 'paragraphMath': {
      const l = pick(r, LATEX);
      return sch.nodes.paragraph.create(null, [
        sch.text(`${word(r)} $`),
        sch.nodes.math_inline.create({ latex: l }),
        sch.text(`$ ${word(r)}`),
      ]);
    }
    case 'task':
      return sch.nodes.task_block.create({ checked: r() < 0.5 }, sch.text(sentence(r)));
    case 'code': {
      const lang = pick(r, ['ts', 'mermaid', 'plaintext', 'rust']);
      const body = Array.from({ length: intBetween(r, 1, 3) }, () => sentence(r));
      return sch.nodes.code_block.create({ language: lang }, sch.text(body.join('\n')));
    }
    case 'quote': {
      const inner = Array.from({ length: intBetween(r, 1, 2) }, () =>
        sch.nodes.paragraph.create(null, sch.text(sentence(r))));
      return sch.nodes.blockquote.create(null, inner);
    }
    case 'bullet':
    case 'ordered': {
      const items = Array.from({ length: intBetween(r, 1, 3) }, () =>
        sch.nodes.list_item.create(null, sch.nodes.paragraph.create(null, sch.text(sentence(r)))));
      return (kind === 'bullet' ? sch.nodes.bullet_list : sch.nodes.ordered_list).create(null, items);
    }
    case 'hr':
      return sch.nodes.horizontal_rule.create();
    case 'table': {
      const cols = intBetween(r, 2, 3);
      const mk = (header: boolean) =>
        sch.nodes[header ? 'table_header' : 'table_row'].create(
          null,
          Array.from({ length: cols }, () =>
            sch.nodes.table_cell.create(null, sch.nodes.paragraph.create(null, sch.text(word(r)))),
          ),
        );
      return sch.nodes.table.create(null, [
        mk(true),
        ...Array.from({ length: intBetween(r, 1, 2) }, () => mk(false)),
      ]);
    }
    case 'mathblock':
      return sch.nodes.math_block.create({ latex: pick(r, LATEX) });
  }
}

/** 生成随机块树（规避相邻同型列表——allowlist 严格面零差异）。 */
function genDoc(r: Rng, count: number): ProseNode[] {
  const nodes: ProseNode[] = [];
  let prev = '';
  for (let i = 0; i < count; i += 1) {
    let n = genBlock(r);
    let guard = 0;
    // 相邻同型列表重抽（解析器天然合并——语义差异面,生成器规避）
    while (
      guard < 8 &&
      ((n.type.name === 'bullet_list' && prev === 'bullet_list') ||
        (n.type.name === 'ordered_list' && prev === 'ordered_list'))
    ) {
      n = genBlock(r);
      guard += 1;
    }
    nodes.push(n);
    prev = n.type.name;
  }
  return nodes;
}

/** 顶层块 → 规范 token（结构等价比较面:类型+关键属性+文本）。 */
function canon(node: ProseNode): string {
  const t = node.textContent;
  switch (node.type.name) {
    case 'heading': return `h${node.attrs.level}:${t}`;
    case 'paragraph': return `p:${t}`;
    case 'task_block': return `task:${node.attrs.checked ? 1 : 0}:${t}`;
    case 'code_block': return `code:${node.attrs.language}:${t}`;
    case 'blockquote':
      return `quote:[${(node.content.content as ProseNode[]).map((c) => c.textContent).join('|')}]`;
    case 'bullet_list':
    case 'ordered_list':
      return `${node.type.name}:[${(node.content.content as ProseNode[]).map((li) => li.textContent).join('|')}]`;
    case 'horizontal_rule': return 'hr';
    case 'table': {
      const rows = (node.content.content as ProseNode[]).map((row) =>
        (row.content.content as ProseNode[]).map((c) => c.textContent).join(','),
      );
      return `table:[${rows.join(';')}]`;
    }
    case 'math_block': return `math:${node.attrs.latex}`;
    default: return `${node.type.name}:${t}`;
  }
}

describe('A-03 property-based md 往返（随机块树 ×100）', () => {
  it('blocks → docToMd → mdToNodes 结构等价（零 allowlist 差异）', () => {
    const failures: string[] = [];
    for (let seed = 1; seed <= 100; seed += 1) {
      const r = mulberry32(seed);
      const blocks = genDoc(r, intBetween(r, 1, 8));
      const doc = auroraSchema.nodes.doc.create(null, blocks);
      const md = docToMd(doc);
      const back = mdToNodes(auroraSchema, md);
      const a = blocks.map(canon);
      const b = back.map(canon);
      if (JSON.stringify(a) !== JSON.stringify(b)) {
        failures.push(`seed=${seed}\n  期: ${JSON.stringify(a)}\n  得: ${JSON.stringify(b)}\n  md: ${JSON.stringify(md)}`);
      }
    }
    expect(failures.join('\n')).toBe('');
  });

  it('DoD 面: 表格/公式/Mermaid/任务块单例往返不丢块型', () => {
    const md = [
      '| a | b |',
      '| --- | --- |',
      '| 1 | 2 |',
      '',
      '$$E=mc^2$$',
      '',
      '```mermaid',
      'graph TD; A-->B;',
      '```',
      '',
      '- [x] 完成',
      '- [ ] 待办',
    ].join('\n');
    const back = mdToNodes(auroraSchema, md);
    const types = back.map((n) => n.type.name);
    expect(types).toContain('table');
    expect(types).toContain('math_block');
    expect(types).toContain('code_block');
    expect(back.some((n) => n.type.name === 'code_block' && n.attrs.language === 'mermaid')).toBe(true);
    expect(types).toContain('task_block');
    // h4-h6 往返保块型（A-03 解析侧补齐）
    const h = mdToNodes(auroraSchema, '#### 深层标题\n##### 五级\n###### 六级');
    expect(h.map((n) => `${n.type.name}${n.attrs?.level ?? ''}`)).toEqual(['heading4', 'heading5', 'heading6']);
  });
});
