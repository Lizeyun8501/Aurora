/** DK-2.7 切片二：GFM 表格往返单测（解析/序列化/转义容忍——vitest）。 */
import { describe, it, expect } from 'vitest';
import { auroraSchema } from '../schema/auroraSchema';
import { mdToNodes, docToMd, mdToDoc } from '../editors/auroraMarkdown';

describe('GFM 表格往返（DK-2.7 切片二）', () => {
  it('mdToNodes：| 行+分隔行解析为 table 子树', () => {
    const md = '前文\n| 名称 | 值 |\n| --- | --- |\n| a | 1 |\n| b | 2 |\n\n后文';
    const nodes = mdToNodes(auroraSchema, md);
    const table = nodes.find((n) => n.type.name === 'table');
    expect(table).toBeTruthy();
    const rows = table!.content.content;
    expect(rows).toHaveLength(3); // header 作首行（v1 约定）+ 2 数据行，分隔行不计
    expect(rows[0].content.content[0].textContent).toBe('名称');
    expect(rows[1].content.content[0].textContent).toBe('a');
    expect(nodes.some((n) => n.type.name === 'paragraph' && n.textContent === '后文')).toBe(true);
  });

  it('docToMd：table 子树 → GFM（首行+分隔行补齐）', () => {
    const doc = mdToDoc(auroraSchema, '| a | b |\n| --- | --- |\n| 1 | 2 |');
    const md = docToMd(doc);
    expect(md).toContain('| a | b |');
    expect(md).toContain('| --- | --- |');
    expect(md).toContain('| 1 | 2 |');
  });

  it('往返闭环：解析再序列化语义恒等', () => {
    const md = '| 名称 | 值 |\n| --- | --- |\n| a | 1 |';
    const doc = mdToDoc(auroraSchema, md);
    const out = docToMd(doc);
    const reparsed = mdToNodes(auroraSchema, out);
    const table = reparsed.find((n) => n.type.name === 'table');
    expect(table!.content.content).toHaveLength(2); // header+1 数据行
    expect(table!.content.content[0].content.content[0].textContent).toBe('名称');
  });

  it('容忍：非 GFM | 行不误判（无分隔行确认）', () => {
    const nodes = mdToNodes(auroraSchema, '| a | b |（无分隔行）');
    expect(nodes.find((n) => n.type.name === 'table')).toBeUndefined();
  });

  it('切片四：header 作 table_header 首行+存量全 table_row 兼容', () => {
    // 新文档：首行 table_header（rowish group）
    const doc = mdToDoc(auroraSchema, '| a | b |\n| --- | --- |\n| 1 | 2 |');
    const table = doc.content.content.find((n) => n.type.name === 'table')!;
    expect(table.content.content[0].type.name).toBe('table_header');
    expect(table.content.content[1].type.name).toBe('table_row');
    // 存量兼容：手构全 table_row 表（旧文档形态）content 校验通过
    const legacyRow = auroraSchema.nodes.table_row.create(null, [
      auroraSchema.nodes.table_cell.create(null, [
        auroraSchema.nodes.paragraph.create(null, [auroraSchema.text('旧')]),
      ]),
    ]);
    const legacy = auroraSchema.nodes.table.create(null, [legacyRow]);
    expect(legacy.check()).toBeUndefined(); // schema 校验不抛
    // 序列化混排（header+row）恒等
    expect(docToMd(doc)).toContain('| a | b |');
  });
});
