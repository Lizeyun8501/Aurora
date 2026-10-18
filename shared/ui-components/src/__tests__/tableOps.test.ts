/** DK-2.7 切片三：表格行列手术单测——真 PM EditorState+Transaction 验证
 *  （位置算法正确性=本卡核心风险面——四命令全跑 dispatch 断言结构）。 */
import { describe, it, expect } from 'vitest';
import { EditorState, TextSelection } from 'prosemirror-state';
import { auroraSchema } from '../schema/auroraSchema';
import { mdToDoc } from '../editors/auroraMarkdown';
import {
  inTable,
  addRowBelowCmd,
  deleteRowCmd,
  addColRightCmd,
  deleteColCmd,
} from '../editors/RichEditor';

const MD = '| a | b | c |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |';

/** 构造光标在指定 row/col cell 段落文本内的 state。 */
function stateInCell(md: string, row: number, col: number): EditorState {
  const doc = mdToDoc(auroraSchema, md);
  let tableStart = -1;
  doc.descendants((n, pos) => {
    if (tableStart === -1 && n.type.name === 'table') tableStart = pos;
  });
  const table = doc.nodeAt(tableStart)!;
  const rows = table.content.content;
  let pos = tableStart + 1;
  for (let r = 0; r < row; r++) pos += rows[r].nodeSize;
  pos += 1; // 进 row
  pos += rows[row].content.content.slice(0, col).reduce((s, n) => s + n.nodeSize, 0); // 进 col cell
  pos += 2; // cell>paragraph 内容起点（+1 开标记 +1 段落开标记）
  const state = EditorState.create({ doc });
  return state.apply(
    state.tr.setSelection(TextSelection.create(state.doc, Math.min(pos + 1, pos + 2))),
  );
}

describe('表格行列手术（DK-2.7 切片三）', () => {
  it('inTable：光标在 cell 内=真，正文=假', () => {
    expect(inTable(stateInCell(MD, 0, 0))).toBe(true);
    const plain = EditorState.create({ doc: mdToDoc(auroraSchema, '正文段落') });
    expect(inTable(plain)).toBe(false);
  });

  it('addRowBelow：光标行下方插空行（行数+1，新行 cell 全空）', () => {
    const s0 = stateInCell(MD, 0, 1);
    let s1 = null as EditorState | null;
    addRowBelowCmd(s0, (tr) => { s1 = s0.apply(tr); });
    expect(s1).toBeTruthy();
    const t1 = (s1 as EditorState).doc.content.content.find((n) => n.type.name === 'table')!;
    expect(t1.content.content).toHaveLength(4);
    expect(t1.content.content[1].content.content.every((c) => c.textContent === '')).toBe(true);
  });

  it('deleteRow：删光标行（行数-1）+单行表拒绝', () => {
    const s0 = stateInCell(MD, 1, 0);
    let s1 = null as EditorState | null;
    deleteRowCmd(s0, (tr) => { s1 = s0.apply(tr); });
    const t1 = (s1 as EditorState).doc.content.content.find((n) => n.type.name === 'table')!;
    expect(t1.content.content).toHaveLength(2);
    // 单行表拒绝（命令 false=未 dispatch）
    const one = mdToDoc(auroraSchema, '| a |\n| --- |');
    const s2 = EditorState.create({ doc: one });
    const ok = deleteRowCmd(s2);
    expect(ok).toBe(false);
  });

  it('addColRight：全行各插一 cell（列数+1）', () => {
    const s0 = stateInCell(MD, 1, 0);
    let s1 = null as EditorState | null;
    addColRightCmd(s0, (tr) => { s1 = s0.apply(tr); });
    const t1 = (s1 as EditorState).doc.content.content.find((n) => n.type.name === 'table')!;
    expect(t1.content.content.every((r) => r.content.content.length === 4)).toBe(true);
  });

  it('deleteCol：全行删光标列（列数-1）+单列表拒绝', () => {
    const s0 = stateInCell(MD, 1, 1); // 光标 col1('2','5') → 删 col1
    let s1 = null as EditorState | null;
    deleteColCmd(s0, (tr) => { s1 = s0.apply(tr); });
    const t1 = (s1 as EditorState).doc.content.content.find((n) => n.type.name === 'table')!;
    expect(t1.content.content.every((r) => r.content.content.length === 2)).toBe(true);
    expect(t1.content.content[1].content.content.map((c) => c.textContent)).toEqual(['1', '3']);
    // 单列表拒绝（命令 false=未 dispatch）
    const one = mdToDoc(auroraSchema, '| a |\n| --- |\n| 1 |');
    const s2 = EditorState.create({ doc: one });
    expect(deleteColCmd(s2)).toBe(false);
  });
});
