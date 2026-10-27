/**
 * R19 移动端块插入命令层 — insertCommands
 *
 * 任务书：MobileEditorDock/BlockInsertPanel 共用命令映射（view + type → bool）。
 * 命令语义对齐 EditorToolbar 既有惯例（RichEditor.tsx）：
 * - 标题/代码块：setBlockType 就地切换（空段落→目标块）
 * - 列表：wrapInList 包裹（已在同型列表 → no-op 幂等返回 true，防套娃——
 *   PM wrapIn 对已有列表项重复调用是幂等嵌套，prosemirror-schema-list
 *   1.5 无 toggleList，自行判 $from.parent）
 * - 任务块：setBlockType（task_block content=inline* 与 paragraph 同构）
 * - 引述：wrapIn（防套娃同理）
 * - 表格：3×3 复用 insertTableCmd 语义（Transaction 手术，零 prosemirror-tables）
 * - 分割线：replaceSelectionWith（horizontal_rule）
 *
 * moveBlock：块上移/下移（顶层块 swap——doc 顶层两块 delete+insert 重排，
 * 选区锚到移动块新起点）。
 */
import type { EditorView } from 'prosemirror-view';
import type { Transaction } from 'prosemirror-state';
import { setBlockType, wrapIn } from 'prosemirror-commands';
import { TextSelection } from 'prosemirror-state';
import { auroraSchema } from '../schema/auroraSchema';
import { makeToggleListCmd } from './RichEditor';

export type BlockKind =
  | 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6'
  | 'bullet' | 'ordered' | 'task' | 'quote' | 'code'
  | 'table' | 'divider';

/** 光标是否在列表内（bullet_list/ordered_list 祖先）。 */
function inList(state: EditorView['state']): boolean {
  const { $from } = state.selection;
  for (let d = $from.depth; d > 0; d--) {
    const name = $from.node(d).type.name;
    if (name === 'bullet_list' || name === 'ordered_list') return true;
  }
  return false;
}

/**
 * 列表内插入新顶层块：当前光标所在顶层块（整个列表）之后插兄弟块，
 * 选区移入新块。语义=「插入块」面板在列表场景的自然行为
 * （列表内的 textblock 转换受 schema content 约束不可行——list_item
 * content=paragraph+，heading/task_block/code_block 非法子节点）。
 */
function insertSiblingBlock(view: EditorView, node: import('prosemirror-model').Node): boolean {
  const { state } = view;
  const { $from } = state.selection;
  const topStart = $from.before(1);
  const topNode = state.doc.resolve(topStart).nodeAfter;
  if (!topNode) return false;
  const pos = topStart + topNode.nodeSize;
  const tr = state.tr.insert(pos, node);
  tr.setSelection(TextSelection.near(tr.doc.resolve(pos + 1)));
  view.dispatch(tr.scrollIntoView());
  view.focus();
  return true;
}

/** PMCommand 包装：派发+回焦（面板关闭后编辑器焦点恢复兜底）。 */
function run(view: EditorView, cmd: (state: EditorView['state'], dispatch?: (tr: Transaction) => void, view?: EditorView) => boolean): boolean {
  const ok = cmd(view.state, view.dispatch, view);
  if (ok) view.focus();
  return ok;
}

/** 插入/切换块类型（命令映射统一入口）。返回是否已派发（false=场景不适用）。
 *
 * 语义（思源对标·插入面板）：
 * - 顶层 textblock（段落/标题/代码）→ 就地转换（setBlockType）
 * - 列表内 → 插新顶层块（列表整体之后插兄弟——schema 约束下唯一正确路径）
 * - bullet/ordered → makeToggleListCmd 三段语义（同型退出/异型切换/非列表包裹）
 */
export function insertBlock(view: EditorView, kind: BlockKind): boolean {
  const schema = auroraSchema;
  const { nodes } = schema;
  const listCtx = inList(view.state);

  switch (kind) {
    case 'h1':
    case 'h2':
    case 'h3':
    case 'h4':
    case 'h5':
    case 'h6': {
      const heading = nodes.heading.create({ level: Number(kind.slice(1)) });
      if (listCtx) return insertSiblingBlock(view, heading);
      return run(view, setBlockType(nodes.heading, { level: Number(kind.slice(1)) }));
    }
    case 'bullet':
      // 三段语义：同型退出/异型就地切换/非列表包裹（RichEditor 现成惯例）
      return run(view, makeToggleListCmd('bullet_list'));
    case 'ordered':
      return run(view, makeToggleListCmd('ordered_list'));
    case 'task': {
      const task = nodes.task_block.create({ checked: false, task_id: null });
      if (listCtx) return insertSiblingBlock(view, task);
      return run(view, setBlockType(nodes.task_block, { checked: false, task_id: null }));
    }
    case 'quote': {
      if (listCtx) {
        const quote = nodes.blockquote.create(null, nodes.paragraph.create());
        return insertSiblingBlock(view, quote);
      }
      return run(view, wrapIn(nodes.blockquote));
    }
    case 'code': {
      if (listCtx) {
        const code = nodes.code_block.create();
        return insertSiblingBlock(view, code);
      }
      return run(view, setBlockType(nodes.code_block));
    }
    case 'table':
      return insertTable(view);
    case 'divider':
      return insertDivider(view);
  }
}

/** 3×3 表格（insertTableCmd 同语义：当前块上方插入）。 */
function insertTable(view: EditorView): boolean {
  const { state } = view;
  const schema = auroraSchema;
  const cell = () => schema.nodes.table_cell.create(null, schema.nodes.paragraph.create());
  const row = () => schema.nodes.table_row.create(null, Array.from({ length: 3 }, cell));
  const table = schema.nodes.table.create(null, Array.from({ length: 3 }, row));
  const { $from } = state.selection;
  const blockDepth = $from.depth - 1;
  if (blockDepth < 0) return false;
  const blockPos = $from.before(blockDepth + 1);
  const insertAt = blockPos + state.doc.resolve(blockPos).node().nodeSize;
  view.dispatch(state.tr.insert(insertAt, table));
  view.focus();
  return true;
}

/** 分割线（insertHorizontalRule 同语义：选区替换）。 */
function insertDivider(view: EditorView): boolean {
  const { state } = view;
  view.dispatch(state.tr.replaceSelectionWith(auroraSchema.nodes.horizontal_rule.create()));
  view.focus();
  return true;
}

/**
 * 顶层块上移/下移（R19 dock 块排序键）。
 *
 * 语义：光标所在顶层块与相邻顶层块 swap（delete 区间 + 按新序插回）。
 * 边界（首块上移/末块下移）返回 false（dock 禁用态依据）。
 * 选区：锚到移动块在新位置起点（TextSelection.near）。
 */
export function moveBlock(view: EditorView, dir: -1 | 1): boolean {
  const { state } = view;
  const { $from } = state.selection;
  if ($from.depth < 1) return false;

  // 收集顶层块 [pos, node]（doc 顶层遍历——与 focusBlock 的 offset 语义同源）
  const tops: Array<{ pos: number; node: import('prosemirror-model').Node }> = [];
  state.doc.forEach((node, offset) => tops.push({ pos: offset, node }));

  // 光标所在顶层块 index（$from.before(1) = 顶层块起点）
  const selfPos = $from.before(1);
  const idx = tops.findIndex((t) => t.pos === selfPos);
  if (idx < 0) return false;
  const target = idx + dir;
  if (target < 0 || target >= tops.length) return false;

  const a = tops[idx];
  const b = tops[target];
  // 区间 [min.pos, max.pos + max.node.nodeSize) 一次删除，按反转序插回
  const lo = dir === 1 ? a : b;
  const hi = dir === 1 ? b : a;
  const rangeStart = lo.pos;
  const rangeEnd = hi.pos + hi.node.nodeSize;
  const tr = state.tr.delete(rangeStart, rangeEnd);
  // swap = 两块顺序反转（A 前 B 后 → B 前 A 后）
  tr.insert(rangeStart, hi.node).insert(rangeStart + hi.node.nodeSize, lo.node);
  // 选区锚到移动块（a 块）新起点
  const newSelfPos = dir === 1 ? rangeStart + hi.node.nodeSize : rangeStart;
  tr.setSelection(TextSelection.near(tr.doc.resolve(newSelfPos)));
  view.dispatch(tr.scrollIntoView());
  view.focus();
  return true;
}
