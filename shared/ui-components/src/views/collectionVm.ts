/**
 * A-07（TA 2026-11-13 工单）: CollectionVm — 集合/数据库视图共享 VM（双端同源）。
 *
 * 设计（DoD「桌面与移动共享同一 VM」）:
 * - 纯函数域: 列型识别/单元格格式化/比较/筛选/排序/分组/类型转换预览——
 *   零 IO 零 React,桌面 CollectionView 与移动列表视图共同消费
 * - 筛选复用智能文件夹 FilterRule 语义（title_contains/tags_include/
 *   tags_exclude AND 语义）+ 属性条件扩展;多条件 AND/OR 合成器
 * - 多列排序（稳定依次比较）;视图配置（形态/筛选/排序）持久化 DTO
 * - 移动端默认列表视图=性能取舍（表格 contentEditable+浮层在 WebView
 *   触摸面的成本高于收益——ADR 面记录于 A-07 交付报告）
 *
 * 停手边界: Formula 列只读（不参与编辑器）;不做跨集合 Relation 双向同步。
 */

/** 属性定义（core PropertyDefinition serde 外部标签形态的对齐面）。 */
export interface PropDefVm {
  id: string;
  name: string;
  prop_type: unknown;
  required: boolean;
}

/** 行（note_id + 标题 + 属性值袋）。 */
export interface RowVm {
  note_id: string;
  title: string;
  values: Record<string, { value: unknown }>;
}

export const TYPE_LABEL: Record<string, string> = {
  Text: '文本',
  Number: '数字',
  Date: '日期',
  Checkbox: '勾选',
  Select: '单选',
  MultiSelect: '多选',
  Formula: '公式',
  Relation: '关联',
};

/** prop_type 外部标签 → 型键（serde 单元变体=串;Select/MultiSelect=对象）。 */
export function typeKey(pt: unknown): string {
  if (typeof pt === 'string') return pt;
  if (pt && typeof pt === 'object' && !Array.isArray(pt)) {
    const k = Object.keys(pt as Record<string, unknown>)[0];
    return k ?? 'Text';
  }
  return 'Text';
}

/** Select/MultiSelect 选项列（对象形态 [{value,color}] → value 数组）。 */
export function selectOptions(d: PropDefVm): string[] {
  const pt = d.prop_type as Record<string, unknown> | string;
  if (typeof pt === 'object' && pt !== null) {
    const v = (pt as Record<string, unknown>)[typeKey(d.prop_type)];
    if (Array.isArray(v)) {
      return v
        .map((o) =>
          typeof o === 'string'
            ? o
            : typeof (o as Record<string, unknown>)?.value === 'string'
              ? ((o as Record<string, unknown>).value as string)
              : '',
        )
        .filter(Boolean);
    }
  }
  return [];
}

/** 单元格显示格式化（Date 友好短格式;其余 String）。 */
export function formatCell(d: PropDefVm, v: unknown): string {
  if (v == null) return '';
  if (typeKey(d.prop_type) === 'Date') {
    const t = typeof v === 'number' ? v : Date.parse(String(v));
    if (!Number.isNaN(t)) {
      const dt = new Date(t);
      const mm = String(dt.getMonth() + 1).padStart(2, '0');
      const dd = String(dt.getDate()).padStart(2, '0');
      const hh = String(dt.getHours()).padStart(2, '0');
      const mi = String(dt.getMinutes()).padStart(2, '0');
      return `${mm}-${dd} ${hh}:${mi}`;
    }
  }
  return String(v);
}

/** 单元格比较（Number/Date 数值序;Checkbox 布尔序;其余 zh locale）。 */
export function cmpCell(a: unknown, b: unknown, type: string): number {
  if (type === 'Number') {
    const na = parseFloat(String(a));
    const nb = parseFloat(String(b));
    const va = Number.isNaN(na) ? -Infinity : na;
    const vb = Number.isNaN(nb) ? -Infinity : nb;
    return va === vb ? 0 : va < vb ? -1 : 1;
  }
  if (type === 'Date') {
    const toMs = (v: unknown): number =>
      typeof v === 'number' ? v : Date.parse(String(v)) || -Infinity;
    return toMs(a) - toMs(b);
  }
  if (type === 'Checkbox') return (a === true ? 1 : 0) - (b === true ? 1 : 0);
  return String(a ?? '').localeCompare(String(b ?? ''), 'zh-Hans-CN');
}

/** 筛选条件（FilterRule 对齐扩展——kind: title/tag 复用智能文件夹语义;prop 为属性列面）。 */
export interface FilterCond {
  kind: 'title' | 'tag' | 'prop';
  /** title/tag: contains/not;prop: eq/ne/contains/gt/lt。 */
  op: 'contains' | 'not' | 'eq' | 'ne' | 'gt' | 'lt';
  value: string;
  /** prop 条件的列 id。 */
  colId?: string;
}

/** 多条件合成器（AND/OR——空条件列表=全过）。 */
export function matchConds(
  row: RowVm,
  conds: FilterCond[],
  mode: 'and' | 'or',
  defs: PropDefVm[],
  rowTags?: string[],
): boolean {
  if (conds.length === 0) return true;
  const one = (c: FilterCond): boolean => {
    const v = c.value.toLowerCase();
    if (c.kind === 'title') {
      return c.op === 'contains'
        ? row.title.toLowerCase().includes(v)
        : !row.title.toLowerCase().includes(v);
    }
    if (c.kind === 'tag') {
      const tags = rowTags ?? [];
      return c.op === 'contains' ? tags.includes(c.value) : !tags.includes(c.value);
    }
    const def = defs.find((d) => d.id === c.colId);
    if (!def || !c.colId) return true;
    const cell = row.values[c.colId]?.value;
    const sv = String(cell ?? '').toLowerCase();
    switch (c.op) {
      case 'contains': return sv.includes(v);
      case 'not': return !sv.includes(v);
      case 'eq': return sv === v;
      case 'ne': return sv !== v;
      case 'gt': return cmpCell(cell, c.value, typeKey(def.prop_type)) > 0;
      case 'lt': return cmpCell(cell, c.value, typeKey(def.prop_type)) < 0;
      default: return true;
    }
  };
  return mode === 'and' ? conds.every(one) : conds.some(one);
}

/** 多列排序（依次稳定比较;keys 空=原始序）。 */
export function sortRows(
  rows: RowVm[],
  keys: Array<{ col: string; dir: 1 | -1 }>,
  defs: PropDefVm[],
): RowVm[] {
  if (keys.length === 0) return rows;
  return [...rows].sort((a, b) => {
    for (const k of keys) {
      const va = k.col === '__title' ? a.title : a.values[k.col]?.value;
      const vb = k.col === '__title' ? b.title : b.values[k.col]?.value;
      const def = defs.find((d) => d.id === k.col);
      const r = def ? cmpCell(va, vb, typeKey(def.prop_type)) : String(va ?? '').localeCompare(String(vb ?? ''), 'zh-Hans-CN');
      if (r !== 0) return r * k.dir;
    }
    return 0;
  });
}

/** 看板分组（按 Select 列;值 null/''=「未分组」）。 */
export function groupRows(
  rows: RowVm[],
  selDef: PropDefVm,
): Array<{ key: string; label: string; rows: RowVm[] }> {
  const opts = selectOptions(selDef);
  return [
    ...opts.map((o) => ({ key: o, label: o, rows: rows.filter((r) => r.values[selDef.id]?.value === o) })),
    { key: '__none', label: '未分组', rows: rows.filter((r) => { const v = r.values[selDef.id]?.value; return v == null || v === ''; }) },
  ];
}

/** 视图配置（持久化 DTO——dbview:{collection_id} KV 形态）。 */
export interface ViewConfig {
  viewMode: 'table' | 'board' | 'list' | 'gallery';
  filterText: string;
  conds: FilterCond[];
  condMode: 'and' | 'or';
  sortKeys: Array<{ col: string; dir: 1 | -1 }>;
}

export const DEFAULT_VIEW_CONFIG: ViewConfig = {
  viewMode: 'table',
  filterText: '',
  conds: [],
  condMode: 'and',
  sortKeys: [],
};

/** 类型切换转换预览（已有值如何转换——A-07 DoD「转换预览」）。
 * 返回每行转换描述（前 5 条样本+统计）;转换语义与 core PropertyEngine
 * coerce 对齐（Text↔Number 可解析保留,不可解析置空;Select 系保留命中选项）。 */
export function previewConversion(
  rows: RowVm[],
  def: PropDefVm,
  toType: string,
  toOptions: string[],
): { sample: Array<{ title: string; from: string; to: string }>; lostCount: number } {
  const sample: Array<{ title: string; from: string; to: string }> = [];
  let lost = 0;
  for (const r of rows) {
    const v = r.values[def.id]?.value;
    if (v == null || v === '') continue;
    const from = formatCell(def, v);
    let to: string;
    switch (toType) {
      case 'Number': {
        const n = Number(String(v));
        to = Number.isFinite(n) ? String(n) : '（空——不可解析）';
        if (!Number.isFinite(n)) lost++;
        break;
      }
      case 'Checkbox': {
        const b = v === true || String(v) === 'true' || String(v) === '是';
        to = b ? 'true' : 'false';
        break;
      }
      case 'Select':
      case 'MultiSelect': {
        const hits = String(v)
          .split(/[,，]/)
          .map((s) => s.trim())
          .filter((s) => s && toOptions.includes(s));
        to = hits.length ? hits.join(',') : '（空——无命中选项）';
        if (!hits.length) lost++;
        break;
      }
      case 'Date': {
        const t = Date.parse(String(v));
        to = Number.isNaN(t) ? '（空——不可解析）' : new Date(t).toISOString();
        if (Number.isNaN(t)) lost++;
        break;
      }
      default:
        to = String(v);
    }
    if (sample.length < 5) sample.push({ title: r.title, from, to });
  }
  return { sample, lostCount: lost };
}
