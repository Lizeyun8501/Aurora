import { describe, it, expect } from 'vitest';
import { DOMParser, DOMSerializer } from 'prosemirror-model';
import { auroraSchema, AURORA_BLOCK_TYPES } from '../schema/auroraSchema';

/** PM HTML 往返工具（doc→HTML→doc）。 */
const proseDocToHtml = (doc: import('prosemirror-model').Node): string => {
  const frag = DOMSerializer.fromSchema(auroraSchema).serializeFragment(doc.content);
  const div = document.createElement('div');
  div.appendChild(frag);
  return div.innerHTML;
};
const proseHtmlToDoc = (html: string): import('prosemirror-model').Node => {
  const div = document.createElement('div');
  div.innerHTML = html;
  return DOMParser.fromSchema(auroraSchema).parse(div);
};

// DK-0F schema 合并版断言（2026-09-25 editor-uplift）：
// 实战语义（mobile 版）为基线 —— code_block/horizontal_rule/strong/em
// 命名 + addListNodes 列表 + task_block(checked/task_id)/embed(embed_type/url)。

describe('auroraSchema', () => {
  it('defines all standard block node types', () => {
    const nodeTypes = Object.keys(auroraSchema.nodes);
    expect(nodeTypes).toContain('doc');
    expect(nodeTypes).toContain('paragraph');
    expect(nodeTypes).toContain('heading');
    expect(nodeTypes).toContain('code_block');
    expect(nodeTypes).toContain('blockquote');
    expect(nodeTypes).toContain('table');
    expect(nodeTypes).toContain('table_row');
    expect(nodeTypes).toContain('table_cell');
    expect(nodeTypes).toContain('list_item');
    expect(nodeTypes).toContain('horizontal_rule');
    expect(nodeTypes).toContain('text');
  });

  it('injects list nodes via addListNodes (mobile 实战链路)', () => {
    const nodeTypes = Object.keys(auroraSchema.nodes);
    expect(nodeTypes).toContain('bullet_list');
    expect(nodeTypes).toContain('ordered_list');
    expect(nodeTypes).toContain('list_item');
  });

  it('defines all Aurora custom block types', () => {
    const nodeTypes = Object.keys(auroraSchema.nodes);
    expect(nodeTypes).toContain('task_block');
    expect(nodeTypes).toContain('embed');
    expect(nodeTypes).toContain('ai_suggestion');
  });

  it('defines all marks (mobile 实战全量 + highlight)', () => {
    const markTypes = Object.keys(auroraSchema.marks);
    expect(markTypes).toEqual(
      expect.arrayContaining([
        'strong',
        'em',
        'underline',
        'strikethrough',
        'code',
        'link',
        'highlight',
      ]),
    );
  });

  it('task_block has checked/task_id attrs (GTD 双向绑定)', () => {
    const taskBlock = auroraSchema.nodes.task_block;
    expect(taskBlock).toBeDefined();
    expect(taskBlock?.spec.attrs).toHaveProperty('checked');
    expect(taskBlock?.spec.attrs).toHaveProperty('task_id');
  });

  it('embed has embed_type/url attrs (draggable 实战版)', () => {
    const embed = auroraSchema.nodes.embed;
    expect(embed).toBeDefined();
    expect(embed?.spec.attrs).toHaveProperty('embed_type');
    expect(embed?.spec.attrs).toHaveProperty('url');
    expect(embed?.spec.draggable).toBe(true);
  });

  it('ai_suggestion has AI-specific attributes', () => {
    const aiSuggestion = auroraSchema.nodes.ai_suggestion;
    expect(aiSuggestion).toBeDefined();
    expect(aiSuggestion?.spec.attrs).toHaveProperty('suggestionType');
    expect(aiSuggestion?.spec.attrs).toHaveProperty('model');
    expect(aiSuggestion?.spec.attrs).toHaveProperty('accepted');
  });

  it('AURORA_BLOCK_TYPES constants match schema node names', () => {
    expect(AURORA_BLOCK_TYPES.TASK_BLOCK).toBe('task_block');
    expect(AURORA_BLOCK_TYPES.EMBED).toBe('embed');
    expect(AURORA_BLOCK_TYPES.AI_SUGGESTION).toBe('ai_suggestion');
    expect(AURORA_BLOCK_TYPES.PARAGRAPH).toBe('paragraph');
    expect(AURORA_BLOCK_TYPES.HEADING).toBe('heading');
    expect(AURORA_BLOCK_TYPES.CODE_BLOCK).toBe('code_block');
    expect(AURORA_BLOCK_TYPES.HORIZONTAL_RULE).toBe('horizontal_rule');
  });

  it('heading supports levels 1-6', () => {
    const heading = auroraSchema.nodes.heading;
    expect(heading).toBeDefined();
    // parseDOM should have entries for h1 through h6
    expect(heading?.spec.parseDOM?.length).toBe(6);
  });

  it('table_cell supports colspan and rowspan', () => {
    const cell = auroraSchema.nodes.table_cell;
    expect(cell).toBeDefined();
    expect(cell?.spec.attrs).toHaveProperty('colspan');
    expect(cell?.spec.attrs).toHaveProperty('rowspan');
  });

  it('highlight mark supports custom color', () => {
    const highlight = auroraSchema.marks.highlight;
    expect(highlight).toBeDefined();
    expect(highlight?.spec.attrs).toHaveProperty('color');
  });

  // ── VIVO 原子笔记对标（2026-11-01）────────────────────────────

  it('fontColor mark: 定义+HTML 往返（VIVO 原子笔记·文字颜色）', () => {
    const fc = auroraSchema.marks.fontColor;
    expect(fc).toBeDefined();
    expect(fc?.spec.attrs).toHaveProperty('color');
    // toDOM: span+style color
    const doc = auroraSchema.node('doc', null, [
      auroraSchema.node('paragraph', null, [
        auroraSchema.text('红色重点', [auroraSchema.mark('fontColor', { color: '#dc2626' })]),
      ]),
    ]);
    const html = proseDocToHtml(doc);
    // jsdom/浏览器 style 归一: #dc2626 → rgb(220, 38, 38)（视觉等价）
    expect(html).toContain('color: rgb(220, 38, 38)');
    // parseDOM: 回读同色（rgb 归一形态）
    const back = proseHtmlToDoc(html);
    const mark = back.firstChild?.firstChild?.marks.find((m) => m.type.name === 'fontColor');
    expect(mark?.attrs.color).toBe('rgb(220, 38, 38)');
  });

  it('fontSize mark: 定义+HTML 往返（VIVO 原子笔记·字号调节）', () => {
    const fs = auroraSchema.marks.fontSize;
    expect(fs).toBeDefined();
    expect(fs?.spec.attrs).toHaveProperty('size');
    const doc = auroraSchema.node('doc', null, [
      auroraSchema.node('paragraph', null, [
        auroraSchema.text('大字', [auroraSchema.mark('fontSize', { size: '24' })]),
      ]),
    ]);
    const html = proseDocToHtml(doc);
    expect(html).toContain('font-size: 24px');
    const back = proseHtmlToDoc(html);
    const mark = back.firstChild?.firstChild?.marks.find((m) => m.type.name === 'fontSize');
    expect(mark?.attrs.size).toBe('24');
  });

  it('fontSize parseDOM: 非 px 字号不误判（false 拒收）', () => {
    const html = '<p><span style="font-size: 2em">em 字号</span> 后文</p>';
    const back = proseHtmlToDoc(html);
    const spanNode = back.firstChild?.firstChild;
    expect(spanNode?.marks.find((m) => m.type.name === 'fontSize')).toBeUndefined();
  });

  it('fontColor+fontSize+highlight 可叠加（三 mark 正交）', () => {
    const doc = auroraSchema.node('doc', null, [
      auroraSchema.node('paragraph', null, [
        auroraSchema.text('复合', [
          auroraSchema.mark('fontColor', { color: '#2563eb' }),
          auroraSchema.mark('fontSize', { size: '20' }),
          auroraSchema.mark('highlight', { color: '#fef08a' }),
        ]),
      ]),
    ]);
    const html = proseDocToHtml(doc);
    const back = proseHtmlToDoc(html);
    const marks = back.firstChild?.firstChild?.marks ?? [];
    expect(marks.map((m) => m.type.name).sort()).toEqual(['fontColor', 'fontSize', 'highlight']);
  });

  it('旧文档零影响: 无新 mark 的 doc 正常解析（向后兼容）', () => {
    const html = '<p>普通文本</p>';
    const back = proseHtmlToDoc(html);
    const marks = back.firstChild?.firstChild?.marks ?? [];
    expect(marks.length).toBe(0);
  });

  it('tagMark/quoteMark（Aa 面板预设）: class 往返', () => {
    expect(auroraSchema.marks.tagMark).toBeDefined();
    expect(auroraSchema.marks.quoteMark).toBeDefined();
    const doc = auroraSchema.node('doc', null, [
      auroraSchema.node('paragraph', null, [
        auroraSchema.text('标记词', [auroraSchema.mark('tagMark')]),
      ]),
    ]);
    const html = proseDocToHtml(doc);
    expect(html).toContain('tag-mark');
    const back = proseHtmlToDoc(html);
    expect(back.firstChild?.firstChild?.marks.find((m) => m.type.name === 'tagMark')).toBeDefined();
  });

  it('code_block carries language attr (dk05mv 数据链)', () => {
    const codeBlock = auroraSchema.nodes.code_block;
    expect(codeBlock).toBeDefined();
    expect(codeBlock?.spec.attrs).toHaveProperty('language');
    expect(codeBlock?.spec.code).toBe(true);
  });
});
