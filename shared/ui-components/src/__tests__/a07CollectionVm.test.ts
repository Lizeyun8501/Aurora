import { describe, it, expect } from 'vitest';
import {
  typeKey,
  selectOptions,
  formatCell,
  matchConds,
  sortRows,
  groupRows,
  previewConversion,
  DEFAULT_VIEW_CONFIG,
  type PropDefVm,
  type RowVm,
  type FilterCond,
} from '../views/collectionVm';

/**
 * A-07（TA 2026-11-13 工单）行为级断言——共享 VM（双端同源面）。
 */

const P = (id: string, pt: unknown): PropDefVm => ({ id, name: id, prop_type: pt, required: false });
const R = (note_id: string, title: string, values: Record<string, { value: unknown }> = {}): RowVm => ({ note_id, title, values });

describe('collectionVm', () => {
  it('typeKey: serde 外部标签双形态', () => {
    expect(typeKey('Text')).toBe('Text');
    expect(typeKey({ Select: [{ value: 'a', color: null }] })).toBe('Select');
    expect(typeKey({ MultiSelect: [] })).toBe('MultiSelect');
    expect(typeKey(undefined)).toBe('Text');
  });

  it('selectOptions: Select 变体取 value 列表', () => {
    expect(selectOptions(P('s', { Select: [{ value: '待办', color: null }, { value: '完成', color: '#f00' }] }))).toEqual(['待办', '完成']);
  });

  it('matchConds: title 包含 + AND/OR 合成', () => {
    const rows = [R('n1', '周会记录'), R('n2', '购物清单')];
    const cContains: FilterCond = { kind: 'title', op: 'contains', value: '周会' };
    const cNot: FilterCond = { kind: 'title', op: 'not', value: '购物' };
    expect(matchConds(rows[0], [cContains], 'and', [])).toBe(true);
    expect(matchConds(rows[1], [cContains], 'and', [])).toBe(false);
    // AND: 周会 AND 非购物 → n1 true
    expect(matchConds(rows[0], [cContains, cNot], 'and', [])).toBe(true);
    // OR: 任一满足——n1（周会命中 contains）true;n2 两条件均 false
    expect(matchConds(rows[0], [cContains, cNot], 'or', [])).toBe(true);
    expect(matchConds(rows[1], [cContains, cNot], 'or', [])).toBe(false);
    // 空条件=全过
    expect(matchConds(rows[1], [], 'and', [])).toBe(true);
  });

  it('matchConds: prop 条件（eq/gt）', () => {
    const defs = [P('num', 'Number')];
    const r = R('n1', 't', { num: { value: 42 } });
    const eq: FilterCond = { kind: 'prop', op: 'eq', value: '42', colId: 'num' };
    const gt: FilterCond = { kind: 'prop', op: 'gt', value: '10', colId: 'num' };
    const lt: FilterCond = { kind: 'prop', op: 'lt', value: '10', colId: 'num' };
    expect(matchConds(r, [eq], 'and', defs)).toBe(true);
    expect(matchConds(r, [gt], 'and', defs)).toBe(true);
    expect(matchConds(r, [lt], 'and', defs)).toBe(false);
  });

  it('sortRows: 多列稳定排序（首列平→次列决）', () => {
    const defs = [P('g', 'Select'), P('num', 'Number')];
    const rows = [
      R('a', 'A', { g: { value: 'X' }, num: { value: 2 } }),
      R('b', 'B', { g: { value: 'Y' }, num: { value: 9 } }),
      R('c', 'C', { g: { value: 'X' }, num: { value: 1 } }),
    ];
    const out = sortRows(rows, [{ col: 'g', dir: 1 }, { col: 'num', dir: -1 }], defs);
    // X 组内 num 降序 → a(2) 前 c(1) 后;Y 组 b 殿后
    expect(out.map((r) => r.note_id)).toEqual(['a', 'c', 'b']);
  });

  it('groupRows: Select 分组+未分组兜底', () => {
    const def = P('s', { Select: [{ value: '待办', color: null }, { value: '完成', color: null }] });
    const rows = [R('a', 'A', { s: { value: '待办' } }), R('b', 'B', {}), R('c', 'C', { s: { value: '完成' } })];
    const gs = groupRows(rows, def);
    expect(gs.map((g) => g.key)).toEqual(['待办', '完成', '__none']);
    expect(gs[0].rows.map((r) => r.note_id)).toEqual(['a']);
    expect(gs[2].rows.map((r) => r.note_id)).toEqual(['b']);
  });

  it('previewConversion: Text→Number 不可解析计数+样本', () => {
    const def = P('t', 'Text');
    const rows = [R('a', 'A', { t: { value: '42' } }), R('b', 'B', { t: { value: 'x' } }), R('c', 'C', { t: { value: '' } })];
    const p = previewConversion(rows, def, 'Number', []);
    expect(p.lostCount).toBe(1);
    expect(p.sample.length).toBe(2); // 空值跳过
    expect(p.sample[0].to).toBe('42');
    expect(p.sample[1].to).toContain('空');
  });

  it('DEFAULT_VIEW_CONFIG: 表格默认+空筛选排序', () => {
    expect(DEFAULT_VIEW_CONFIG.viewMode).toBe('table');
    expect(DEFAULT_VIEW_CONFIG.conds).toEqual([]);
    expect(DEFAULT_VIEW_CONFIG.sortKeys).toEqual([]);
  });

  it('formatCell: Date 双形态友好化', () => {
    const d = P('dt', 'Date');
    const out = formatCell(d, '2026-11-13T09:30:00Z');
    expect(out).toMatch(/11-13/);
  });
});
