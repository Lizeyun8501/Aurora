/**
 * DK-2.7 切片五：公式 md 往返单测——$...$ 行内 / $$...$$ 块级。
 * 重点：货币不误判（Typora 启发式）+ \$ 转义 + 序列化恒等。
 */
import { describe, expect, it } from 'vitest';
import { auroraSchema } from '../schema/auroraSchema';
import { mdToDoc, docToMd } from '../editors/auroraMarkdown';

const nodeOf = (nodes: ReturnType<typeof mdToDoc>['content']['content']) => nodes;

describe('DK-2.7 切片五：公式', () => {
  it('行内 $...$ 解析为 math_inline+序列化恒等', () => {
    const doc = mdToDoc(auroraSchema, '质能方程 $E=mc^2$ 是物理');
    const para = nodeOf(doc.content.content)[0];
    const inlines = para.content.content.map((n: any) => n.type.name);
    expect(inlines).toEqual(['text', 'math_inline', 'text']);
    expect(para.content.content[1].attrs.latex).toBe('E=mc^2');
    expect(docToMd(doc)).toBe('质能方程 $E=mc^2$ 是物理');
  });

  it('货币 "$5 and $6" 不误判（首尾空白启发式）', () => {
    const doc = mdToDoc(auroraSchema, '价格 $5 and $6 元');
    const para = nodeOf(doc.content.content)[0];
    expect(para.content.content.some((n: any) => n.type.name === 'math_inline')).toBe(false);
    expect(docToMd(doc)).toBe('价格 $5 and $6 元');
  });

  it('\\$ 转义为字面 $', () => {
    const doc = mdToDoc(auroraSchema, '价格 \\$5 稳定');
    const para = nodeOf(doc.content.content)[0];
    expect(para.textContent).toBe('价格 $5 稳定');
  });

  it('块级 $$ 多行解析+序列化还原', () => {
    const md = '$$\n\\int_0^1 x\\,dx = \\frac{1}{2}\n$$';
    const doc = mdToDoc(auroraSchema, md);
    const math = nodeOf(doc.content.content)[0];
    expect(math.type.name).toBe('math_block');
    expect(math.attrs.latex).toContain('\\int_0^1');
    expect(docToMd(doc)).toBe(md);
  });

  it('单行 $$...$$ 块级', () => {
    const doc = mdToDoc(auroraSchema, '$$a^2+b^2=c^2$$');
    expect(nodeOf(doc.content.content)[0].type.name).toBe('math_block');
  });
});
