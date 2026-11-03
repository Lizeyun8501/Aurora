// ProseMirror + Loro CRDT 富文本编辑器 — V19 §35（DEV-009）
//
// 初始化: platform.getNoteSnapshot(noteId) → LoroDoc.import → EditorView
// 保存:   debounce 1s → doc.export snapshot → platform.saveNoteSnapshot
//         （CRDT 合并语义，P2P 对端修改不丢失）
// 降级:   无 Android 桥（浏览器 mock）→ textarea 纯文本编辑
// 工具栏: V12 — 标题/加粗斜体下划线删除线/行内代码/列表/任务/引用/代码块/分割线

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import type { EditorView } from 'prosemirror-view';
import type { EditorState } from 'prosemirror-state';
import { toggleMark, setBlockType, wrapIn } from 'prosemirror-commands';
import { wrapInList, liftListItem } from 'prosemirror-schema-list';

/**
 * 平台快照桥（依赖注入）— mobile 传 androidPlatform，desktop 侧（DK-05）
 * 传等价 adapter。共享层不 import 任何宿主平台模块。
 */
export interface EditorPlatformBridge {
  saveNoteContent(noteId: string, content: string): number | boolean;
  getNoteSnapshot(noteId: string): string;
  saveNoteSnapshot(noteId: string, snapshotBase64: string): boolean;
}
import {
  createAuroraEditor,
  loroDocFromBase64,
  bytesToBase64,
  undo,
  redo,
  AuroraEditorHandle,
} from './auroraEditor';
import { insertMathBlock, insertMermaidBlock } from './mathInputRules';
import { insertBlock } from './insertCommands';
import { auroraSchema } from '../schema/auroraSchema';
import { MobileEditorDock } from './MobileEditorDock';

export type EditorStatus = 'loading' | 'rich' | 'fallback';

interface RichEditorProps {
  noteId: string;
  /** 平台快照桥（宿主注入：mobile=androidPlatform；DK-05 桌面侧等价 adapter）。 */
  platform: EditorPlatformBridge;
  /** 降级用纯文本初值（无桥时）。 */
  fallbackText: string;
  onDirty?: () => void;
  /** 保存完成回调（外层显示"已保存"）。 */
  onSaved?: () => void;
  /** 状态回调（外层显示"编辑器加载中/降级"）。 */
  onStatus?: (s: EditorStatus) => void;
  /** R19：移动端底部编辑 Dock（+插入面板/撤销重做/块上下移）。
   *  宿主显式开启（mobile=MobileApp；desktop 不传=默认关）。 */
  mobileDock?: boolean;
}

// ---------------------------------------------------------------------------
// 降级模式 Markdown 工具条 — V23-I5 清单②（对齐富文本工具栏能力面）
// 受控 textarea: 全部走 setState, 光标经 rAF 恢复; 行前缀切换 + 选区包裹
// ---------------------------------------------------------------------------

/** 行前缀操作结果（text 新文本 / caret 新光标位）。 */
interface TextOp {
  text: string;
  caret: number;
}

/** 标题/列表/引用行前缀切换: 已有同级前缀 → 剥除回正文; 其他 # 级 → 替换。 */
function toggleLinePrefix(v: string, selStart: number, prefix: string | null): TextOp {
  const lineStart = v.lastIndexOf('\n', selStart - 1) + 1;
  const nlAt = v.indexOf('\n', selStart);
  const lineEnd = nlAt === -1 ? v.length : nlAt;
  const line = v.slice(lineStart, lineEnd);
  const bare = line.replace(/^#{1,6} |^[-*+] |^\d{1,9}\. |^> /, '');
  const newLine = prefix === null ? bare : prefix + bare;
  return {
    text: v.slice(0, lineStart) + newLine + v.slice(lineEnd),
    caret: lineStart + Math.max(newLine.length, 0),
  };
}

/** 选区包裹: 对称存在同标记 → 解开; 否则包裹（空选区填占位词）。 */
function wrapSelection(v: string, s: number, e: number, mark: string): TextOp {
  const before = v.slice(Math.max(0, s - mark.length), s);
  const after = v.slice(e, e + mark.length);
  if (before === mark && after === mark) {
    const inner = v.slice(s, e);
    return {
      text: v.slice(0, s - mark.length) + inner + v.slice(e + mark.length),
      caret: s - mark.length + inner.length,
    };
  }
  const inner = v.slice(s, e) || '文本';
  return {
    text: v.slice(0, s) + mark + inner + mark + v.slice(e),
    caret: s + mark.length + inner.length,
  };
}

// ---------------------------------------------------------------------------
// 工具栏命令（view 为空时按钮禁用）— PM 命令统一 (state, dispatch) 签名
// ---------------------------------------------------------------------------

type PMCommand = (
  state: EditorState,
  dispatch?: ((tr: import('prosemirror-state').Transaction) => void) | undefined,
  view?: EditorView,
) => boolean;

function runCmd(view: EditorView | null, cmd: PMCommand) {
  if (!view) return;
  cmd(view.state, view.dispatch);
  // E13（TA 17:56）: 撤 OK→focus——面板项应用命令不抢焦点（输入法不跳）;dispatch 即生效,
  // 恢复输入法仅发生在「关面板」链（restoreKbdAfterPanel: 编辑区点击/栏钮 toggle）。
}

function insertHorizontalRule(view: EditorView | null) {
  if (!view) return;
  const { state } = view;
  const tr = state.tr.replaceSelectionWith(auroraSchema.nodes.horizontal_rule.create());
  view.dispatch(tr); // E13: 不 focus——插入命令不弹输入法
}
// 注: hr 按钮经 btn() 的 onClick 调 insertHorizontalRule(view)，与 runCmd 分离

/**
 * 列表切换 — 主流编辑器语义（非嵌套）:
 *  1. 光标已在目标类型列表内 → liftListItem 退出列表
 *  2. 光标在另一类型列表内（ul↔ol）→ setNodeMarkup 就地切换列表类型
 *  3. 非列表 → wrapInList 包裹
 * （prosemirror-schema-list 1.5 无 toggleList，自行组合）
 */
export function makeToggleListCmd(
  listType: 'bullet_list' | 'ordered_list',
): PMCommand {
  const itemType = auroraSchema.nodes.list_item;
  const target = auroraSchema.nodes[listType];
  return (state, dispatch) => {
    const { $from } = state.selection;
    for (let d = $from.depth; d >= 0; d--) {
      const n = $from.node(d);
      if (n.type === target) {
        // 同类型 → 退出
        return liftListItem(itemType)(state, dispatch);
      }
      if (n.type.name === 'bullet_list' || n.type.name === 'ordered_list') {
        // 异类型 → 就地切换（bullet_list/ordered_list 的 content 均为 list_item+，兼容）
        if (dispatch) {
          dispatch(state.tr.setNodeMarkup($from.before(d), target).scrollIntoView());
        }
        return true;
      }
    }
    return wrapInList(target)(state, dispatch);
  };
}

/** 计算工具栏激活态（tick 变化触发重算）。 */
export function useToolbarState(view: EditorView | null, _tick: number) {
  if (!view) {
    return { marks: new Set<string>(), blocks: new Set<string>(), canUndo: false, canRedo: false };
  }
  const { state } = view;
  const marks = new Set<string>();
  const selMarks = state.storedMarks ?? state.selection.$from.marks();
  for (const m of selMarks) marks.add(m.type.name);

  const blocks = new Set<string>();
  const { $from } = state.selection;
  for (let d = $from.depth; d >= 0; d--) {
    const n = $from.node(d);
    if (n.type.name === 'heading') blocks.add(`heading-${n.attrs.level}`);
    else blocks.add(n.type.name);
  }

  // PM 命令约定: dispatch 传 undefined 只检测可执行性
  const canUndo = (() => {
    try {
      return undo(state, undefined);
    } catch {
      return false;
    }
  })();
  const canRedo = (() => {
    try {
      return redo(state, undefined);
    } catch {
      return false;
    }
  })();

  return { marks, blocks, canUndo, canRedo };
}

// ---------------------------------------------------------------------------
// 图文模板（VIVO 原子笔记对标 v3·2026-11-01）— 预设块序列光标前插入
// ---------------------------------------------------------------------------

type TemplateKey = 'diary' | 'meeting' | 'todo' | 'reading';

const TEMPLATES: Array<{ key: TemplateKey; icon: string; title: string }> = [
  { key: 'diary', icon: '📅', title: '日记' },
  { key: 'meeting', icon: '🗓️', title: '会议纪要' },
  { key: 'todo', icon: '✅', title: '待办清单' },
  { key: 'reading', icon: '📚', title: '读书笔记' },
];

/** 模板 → 块节点序列（p/h2/task/blockquote/hr 组合——纯 schema 既有能力零新面）。 */
function templateNodes(key: TemplateKey, schema: typeof auroraSchema): Array<import('prosemirror-model').Node> {
  const { nodes } = schema;
  // text 不可 destructure 出同名箭头再调用——遮蔽后 t 无 .nodes（self-call trap 实证）。
  // 直接捕获 schema.text（Schema.text(text, marks) 造 text node）。
  const p = (t: string) => nodes.paragraph.create(null, schema.text(t));
  const h2 = (t: string) => nodes.heading.create({ level: 2 }, schema.text(t));
  const task = (t: string) => nodes.task_block.create({ checked: false, task_id: null }, schema.text(t));
  switch (key) {
    case 'diary':
      return [h2('📅 日记'), p('天气 / 心情：'), p('今日记录：'), nodes.horizontal_rule.create(), task('今日一件小事')];
    case 'meeting':
      return [h2('🗓️ 会议纪要'), p('参会人：'), p('议题：'), p('结论：'), nodes.horizontal_rule.create(), task('行动项 1'), task('行动项 2')];
    case 'todo':
      return [h2('✅ 待办清单'), task('事项 1'), task('事项 2'), task('事项 3')];
    case 'reading':
      return [h2('📚 读书笔记'), p('书名 / 作者：'), nodes.blockquote.create(null, nodes.paragraph.create(null, schema.text('摘录…'))), p('笔记：'), p('★ 评分：')];
  }
}

/** 模板插入：光标所在块前整序列插入（insertMathBlock 同位语义——$from.before）。 */
export function insertTemplate(view: EditorView | null, key: TemplateKey): boolean {
  if (!view) return false;
  const { state } = view;
  const { $from } = state.selection;
  const pos = $from.before($from.depth);
  const frags = templateNodes(key, view.state.schema as typeof auroraSchema);
  if (!frags.length) return false;
  const tr = state.tr;
  let at = pos;
  for (const n of frags) { tr.insert(at, n); at += n.nodeSize; }
  view.dispatch(tr.scrollIntoView()); // E13: 不 focus——模板插入不弹输入法
  return true;
}

// 工具栏按钮定义
type TBCmd = PMCommand;

const MARK_BTNS: Array<{ key: string; label: string; title: string; cmd: TBCmd }> = [
  { key: 'strong', label: 'B', title: '加粗', cmd: toggleMark(auroraSchema.marks.strong) },
  { key: 'em', label: 'I', title: '斜体', cmd: toggleMark(auroraSchema.marks.em) },
  { key: 'underline', label: 'U', title: '下划线', cmd: toggleMark(auroraSchema.marks.underline) },
  {
    key: 'strikethrough',
    label: 'S',
    title: '删除线',
    cmd: toggleMark(auroraSchema.marks.strikethrough),
  },
  { key: 'code', label: '</>', title: '行内代码', cmd: toggleMark(auroraSchema.marks.code) },
];

function blockCmd(type: string, attrs?: Record<string, unknown>): TBCmd {
  const nodeType = auroraSchema.nodes[type];
  return (state, dispatch) => {
    const { $from } = state.selection;
    // 再次点击同类型 → 回到正文段落
    const cur = $from.parent;
    if (cur.type.name === type && (!attrs || cur.attrs.level === attrs.level)) {
      return setBlockType(auroraSchema.nodes.paragraph)(state, dispatch);
    }
    return setBlockType(nodeType, attrs)(state, dispatch);
  };
}

/** DK-2.7 切片二：插入 3×3 表格（光标所在块后插入——table 子树手工构建，
 * 不依赖 prosemirror-tables——零依赖纪律；行列手术 v2 派单）。 */
function insertTableCmd(
  state: EditorState,
  dispatch?: (tr: import('prosemirror-state').Transaction) => void,
): boolean {
  const schema = auroraSchema;
  const cell = () => schema.nodes.table_cell.create(null, schema.nodes.paragraph.create());
  const row = () => schema.nodes.table_row.create(null, Array.from({ length: 3 }, cell));
  const table = schema.nodes.table.create(null, Array.from({ length: 3 }, row));
  if (dispatch) {
    const { $from } = state.selection;
    const blockDepth = $from.depth - 1; // 当前块（paragraph 等）上方
    const blockPos = $from.before(blockDepth + 1);
    dispatch(state.tr.insert(blockPos + state.doc.resolve(blockPos).node().nodeSize, table));
  }
  return true;
}

// ---------------------------------------------------------------------------
// DK-2.7 切片三：表格行列手术（零 prosemirror-tables——Transaction 手术四命令）
// ---------------------------------------------------------------------------

/** 光标是否在 table 内（doc>table>row>cell>para 链——depth 5）。 */
export function inTable(state: EditorState): boolean {
  const { $from } = state.selection;
  let depth = $from.depth;
  while (depth > 0) {
    if ($from.node(depth).type.name === 'table') return true;
    depth -= 1;
  }
  return false;
}

/** 光标定位三元组：table 节点+row 序号+cell 序号（不在表内返回 null）。 */
export function tableCtx(state: EditorState): {
  table: import('prosemirror-model').Node;
  tablePos: number;
  rowIdx: number;
  cellIdx: number;
} | null {
  const { $from } = state.selection;
  let depth = $from.depth;
  while (depth > 0 && $from.node(depth).type.name !== 'table') depth -= 1;
  if (depth === 0) return null;
  const table = $from.node(depth);
  // row/cell 序号：相对 table 层的 index 链（node(depth+1)=row, node(depth+2)=cell）
  const rowIdx = $from.index(depth); // table 下第几个 row
  const cellIdx = $from.index(depth + 1); // row 下第几个 cell
  return { table, tablePos: $from.before(depth), rowIdx, cellIdx };
}

export function addRowBelowCmd(state: EditorState, dispatch?: (tr: import('prosemirror-state').Transaction) => void): boolean {
  const ctx = tableCtx(state);
  if (!ctx) return false;
  const row = ctx.table.content.content[ctx.rowIdx];
  // 克隆行结构、清空 cell 内容（新行=空段落 cell）
  const cells = row.content.content.map(() =>
    auroraSchema.nodes.table_cell.create(null, auroraSchema.nodes.paragraph.create()),
  );
  const newRow = auroraSchema.nodes.table_row.create(null, cells);
  if (dispatch) {
    const rowPos = ctx.tablePos + 1; // table 内容起点
    let insertAt = rowPos;
    for (let i = 0; i <= ctx.rowIdx; i++) insertAt += ctx.table.content.content[i].nodeSize;
    dispatch(state.tr.insert(insertAt, newRow));
  }
  return true;
}

export function deleteRowCmd(state: EditorState, dispatch?: (tr: import('prosemirror-state').Transaction) => void): boolean {
  const ctx = tableCtx(state);
  if (!ctx || ctx.table.content.content.length <= 1) return false; // 保至少 1 行
  if (dispatch) {
    const row = ctx.table.content.content[ctx.rowIdx];
    const rowPos = ctx.tablePos + 1 + ctx.table.content.content
      .slice(0, ctx.rowIdx)
      .reduce((s, n) => s + n.nodeSize, 0);
    dispatch(state.tr.delete(rowPos, rowPos + row.nodeSize));
  }
  return true;
}

export function addColRightCmd(state: EditorState, dispatch?: (tr: import('prosemirror-state').Transaction) => void): boolean {
  const ctx = tableCtx(state);
  if (!ctx) return false;
  if (dispatch) {
    const tr = state.tr;
    // 逐行倒序插入（位置不因前插偏移）
    const rows = ctx.table.content.content;
    for (let r = rows.length - 1; r >= 0; r--) {
      const row = rows[r];
      const cells = row.content.content;
      const cell = cells[Math.min(ctx.cellIdx, cells.length - 1)];
      const cellStart =
        ctx.tablePos + 1 + rows.slice(0, r).reduce((s, n) => s + n.nodeSize, 0) + 1
        + cells.slice(0, Math.min(ctx.cellIdx, cells.length - 1)).reduce((s, n) => s + n.nodeSize, 0)
        + cell.nodeSize;
      tr.insert(
        cellStart,
        auroraSchema.nodes.table_cell.create(null, auroraSchema.nodes.paragraph.create()),
      );
    }
    dispatch(tr);
  }
  return true;
}

export function deleteColCmd(state: EditorState, dispatch?: (tr: import('prosemirror-state').Transaction) => void): boolean {
  const ctx = tableCtx(state);
  if (!ctx) return false;
  const rows = ctx.table.content.content;
  if (rows[0].content.content.length <= 1) return false; // 保至少 1 列
  if (dispatch) {
    const tr = state.tr;
    for (let r = rows.length - 1; r >= 0; r--) {
      const cells = rows[r].content.content;
      if (ctx.cellIdx >= cells.length) continue; // 短行容错
      const cell = cells[ctx.cellIdx];
      const cellStart =
        ctx.tablePos + 1 + rows.slice(0, r).reduce((s, n) => s + n.nodeSize, 0) + 1
        + cells.slice(0, ctx.cellIdx).reduce((s, n) => s + n.nodeSize, 0);
      tr.delete(cellStart, cellStart + cell.nodeSize);
    }
    dispatch(tr);
  }
  return true;
}

const TABLE_BTNS: Array<{ label: string; title: string; cmd: TBCmd }> = [
  { label: '＋行', title: '下方插入行', cmd: addRowBelowCmd },
  { label: '－行', title: '删除当前行', cmd: deleteRowCmd },
  { label: '＋列', title: '右侧插入列', cmd: addColRightCmd },
  { label: '－列', title: '删除当前列', cmd: deleteColCmd },
];

const BLOCK_BTNS: Array<{ key: string; label: string; title: string; cmd: TBCmd }> = [
  { key: 'heading-1', label: 'H1', title: '标题 1', cmd: blockCmd('heading', { level: 1 }) },
  { key: 'heading-2', label: 'H2', title: '标题 2', cmd: blockCmd('heading', { level: 2 }) },
  { key: 'heading-3', label: 'H3', title: '标题 3', cmd: blockCmd('heading', { level: 3 }) },
  { key: 'code_block', label: '{ }', title: '代码块', cmd: blockCmd('code_block') },
  { key: 'task_block', label: '☑', title: '任务', cmd: blockCmd('task_block') },
  { key: 'table', label: '▦', title: '表格（3×3）', cmd: insertTableCmd },
];

const LIST_BTNS: Array<{ key: string; label: string; title: string; cmd: TBCmd }> = [
  {
    key: 'bullet_list',
    label: '•≡',
    title: '无序列表',
    cmd: makeToggleListCmd('bullet_list'),
  },
  {
    key: 'ordered_list',
    label: '1≡',
    title: '有序列表',
    cmd: makeToggleListCmd('ordered_list'),
  },
  {
    key: 'blockquote',
    label: '❝',
    title: '引用',
    cmd: wrapIn(auroraSchema.nodes.blockquote),
  },
];

// ---------------------------------------------------------------------------
// 工具栏组件
// ---------------------------------------------------------------------------


/**
 * 选中浮动菜单 — DK-05M: 选区非空时显示于选区上方（不遮挡）。
 * selectionchange 驱动；组合输入期间隐藏（IME 候选优先）。
 */
function FloatingMenu({ view }: { view: EditorView | null }) {
  const [rect, setRect] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    if (!view) return;
    let raf = 0;
    // DK-05M 裁决③：rAF 节流统一三源（selectionchange/scroll/visualViewport resize）
    const schedule = () => {
      if (!raf) raf = requestAnimationFrame(update);
    };
    const update = () => {
      raf = 0;
      // 组合输入期间隐藏（IME 候选浮层优先, 避免双重浮层遮挡）
      if (view.composing) {
        setRect(null);
        return;
      }
      const { state } = view;
      if (state.selection.empty) {
        setRect(null);
        return;
      }
      const coords = view.coordsAtPos(state.selection.from);
      if (!view.dom.parentElement) return;
      // 视口系钳制（裁决③：visualViewport 修正 + 遮挡时贴工具条上方）
      // vv 可视下界：真实环境 = vv.height + vv.offsetTop；verify mock 只改 height
      const vv = window.visualViewport;
      const vvBottom = vv ? vv.height + vv.offsetTop : window.innerHeight;
      const vvTop = vv ? vv.offsetTop : 0;
      const TOOLBAR_H = 48; // .editor-toolbar 实测视觉高度（40 热区 + padding）
      const MENU_H = 34;
      let topView = coords.top - 46; // 菜单置于选区上方 46px（原偏移语义）
      // 遮挡/越界 → 上缘钳到工具条上方（bottom=--kbd-inset 态下工具条上缘≈视口下界-工具条高）
      const floor = vvBottom - TOOLBAR_H - MENU_H - 4;
      topView = Math.max(vvTop + 4, Math.min(topView, floor - MENU_H));
      // DK-05M：菜单改 fixed 视口系定位（坐标即视口坐标；键盘态/滚动钳制天然一致）
      setRect({
        top: topView,
        left: Math.max(8, Math.min(coords.left, (vv ? vv.width : window.innerWidth) - 200)),
      });
    };
    document.addEventListener('selectionchange', schedule);
    window.addEventListener('scroll', schedule, true);
    window.visualViewport?.addEventListener('resize', schedule, { passive: true });
    return () => {
      if (raf) cancelAnimationFrame(raf);
      document.removeEventListener('selectionchange', schedule);
      window.removeEventListener('scroll', schedule, true);
      window.visualViewport?.removeEventListener('resize', schedule);
    };
  }, [view]);

  if (!view || !rect) return null;
  const btn = (label: string, title: string, onClick: () => void) => (
    <button
      key={title}
      title={title}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {label}
    </button>
  );
  return (
    <div
      className="floating-menu"
      role="menu"
      aria-label="选中快捷格式"
      style={{ top: rect.top, left: rect.left }}
    >
      {btn('B', '加粗', () => runCmd(view, toggleMark(auroraSchema.marks.strong)))}
      {btn('I', '斜体', () => runCmd(view, toggleMark(auroraSchema.marks.em)))}
      {btn('H2', '标题', () =>
        runCmd(view, setBlockType(auroraSchema.nodes.heading, { level: 2 })))}
      {btn('</>', '代码', () =>
        runCmd(view, toggleMark(auroraSchema.marks.code)))}
      {/* 原子笔记对标: 选中即高亮（默认黄） */}
      <button
        title="高亮"
        aria-label="高亮"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => runCmd(view, toggleMark(auroraSchema.marks.highlight, { color: '#fef08a' }))}
        style={{ fontWeight: 700, boxShadow: 'inset 0 -0.45em #fef08a' }}
      >
        ▮
      </button>
      {/* 原子笔记对标: 选中即红色 */}
      <button
        title="红色文字"
        aria-label="红色文字"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => runCmd(view, toggleMark(auroraSchema.marks.fontColor, { color: '#dc2626' }))}
        style={{ color: '#dc2626', fontWeight: 800, textDecoration: 'underline' }}
      >
        A
      </button>
    </div>
  );
}

export function EditorToolbar({ view, tick }: { view: EditorView | null; tick: number }) {
  const { marks, blocks, canUndo, canRedo } = useToolbarState(view, tick);
  // VIVO 原子笔记对标（2026-11-01）：文字色/字号/高亮三弹出组。
  const [pop, setPop] = useState<'color' | 'size' | 'hl' | 'style' | 'symbol' | 'template' | 'block' | 'insert' | null>(null);
  /** Aa 面板 tab（v4 E2: A/A⁺/▮ 三独立组+B·I·U·S 并入「格式」tab——VIVO Aa 一钮弹全部文字格式对标）。 */
  const [styleTab, setStyleTab] = useState<'fmt' | 'style' | 'symbol' | 'template'>('fmt');
  // E12（TA 16:36）: 粘性面板——内容点击不关（连续应用）;编辑区点击/栏钮 toggle 才关。
  // PM dom click 关面板+restore（撤光标避让垫+回焦恢复输入法）。
  useEffect(() => {
    if (!view || !pop) return;
    const dom = view.dom;
    const onEditClick = () => { setPop(null); restoreKbdAfterPanel(); };
    dom.addEventListener('click', onEditClick);
    return () => dom.removeEventListener('click', onEditClick);
  }, [view, pop]);
  // E19（TA 20:51 深扫 S10）: Escape 键盘通道关面板——桌面键盘用户预期;粘性面板
  // 的第三关闭路径（点击编辑区/栏钮/Escape）。
  useEffect(() => {
    if (!pop) return;
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { setPop(null); restoreKbdAfterPanel(); }
    };
    document.addEventListener('keydown', onEsc);
    return () => document.removeEventListener('keydown', onEsc);
  }, [pop]);
  // E11（TA 16:19 裁决「弹窗置于工具栏下方」）: 弹期栏动态上浮——bottom=面板实际高+4 隙;
  // ResizeObserver 随 tab 切换面板高变实时跟（Aa 四 tab 高各异）。
  const [popH, setPopH] = useState(0);
  const popRef = useRef<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    const el = popRef.current;
    if (!el) { setPopH(0); return; }
    const measure = () => setPopH(el.offsetHeight); // offsetHeight=layout 高（scaleY 动画首帧 rect.height=0.72x 会测小——D19 实证 110 vs 153）
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [pop, styleTab]);
  const FONT_COLORS = ['#dc2626', '#ea580c', '#ca8a04', '#16a34a', '#2563eb', '#7c3aed', '#475569'];
  const HL_COLORS = ['#fef08a', '#bbf7d0', '#bae6fd', '#fbcfe8', '#fed7aa', '#e9d5ff'];
  const SIZES: Array<[string, string]> = [['14', 'S'], ['17', 'M'], ['20', 'L'], ['24', 'XL']];
  /**
   * E4 键盘让位（TA 13:53 裁决）: 弹面板→隐藏输入法（面板向下弹获全空间）;
   * 关面板→编辑器曾聚焦则回焦恢复输入法。防重=面板已开不重复 blur。
   */
  const kbdWasActive = useRef(false);
  const hideKbdForPanel = () => {
    if (!view || kbdWasActive.current) return;
    kbdWasActive.current = view.hasFocus();
    // E10a 光标避让（TA 14:03）: 开面板前把光标行滚到面板上方安全区——面板 max 42vh+栏 49px
    // ≈ 底部 47vh 占位;coordsAtPos 判光标视觉位,落底半区则滚 host 使其进入上 53vh 可视带。
    // E10a 双保: ①光标贴底段物理滚不上（滚容器已尽）→PM 尾垫临时加 260px 拉出滚动余量;
    // ②差量精准滚——光标滚至 host 上 30% 带（面板≤42vh+栏 49=底 47vh 占位恒在下方）
    try {
      view.dom.classList.add('e10-cursor-pad');
      const head = view.state.selection.head;
      const host = view.dom.closest('.rich-editor-host') ?? view.dom.parentElement;
      const coords = view.coordsAtPos(head);
      const hostRect = (host as HTMLElement).getBoundingClientRect();
      const target = hostRect.top + hostRect.height * 0.3;
      const delta = coords.top - target;
      if (delta > 0) (host as HTMLElement).scrollTop += delta;
    } catch { /* 空选区防御 */ }
    if (kbdWasActive.current) view.dom.blur(); // PM EditorView 无 blur()——DOM 正法
  };
  const restoreKbdAfterPanel = () => {
    if (view) view.dom.classList.remove('e10-cursor-pad'); // E10a: 撤光标避让尾垫
    if (kbdWasActive.current && view) {
      view.focus();
      // 聚焦即恢复键盘——不 setSelection（保留用户原选区,样式应用链不破坏）
    }
    kbdWasActive.current = false;
  };
  /** 正法清 mark：removeMark（选区范围）;空选区清 storedMarks 由 PM next 命令态兜底。 */
  const clearMark = (markName: 'fontColor' | 'highlight') => {
    if (!view) return;
    const { from, to } = view.state.selection;
    view.dispatch(view.state.tr.removeMark(from, to, auroraSchema.marks[markName]));
   
  };

  const btn = (
    label: string,
    title: string,
    active: boolean,
    onClick: () => void,
    style?: CSSProperties,
  ) => (
    <button
      key={title}
      className={`tb-btn ${active ? 'active' : ''}`}
      title={title}
      onMouseDown={(e) => e.preventDefault()} // 保持编辑器焦点
      onClick={onClick}
      style={style}
    >
      {label}
    </button>
  );

  /* E4: portal 跳出 toolbar stacking 语境（栏 z-80 建 context,面板 z 在其内不越级——
     DOM 移 body 直挂后面板 z-39 与栏 80 顶层平比,栏稳居上可点）+overflow 裁剪免疫（E3 教训） */
  return createPortal(
    <>
    <div
      className={`editor-toolbar ${pop ? 'pop-open' : ''}`}
      role="toolbar" aria-label="格式工具栏"
      style={pop && popH > 0 ? { bottom: `calc(${popH}px + max(var(--kbd-inset, 0px), env(safe-area-inset-bottom)) + 4px)` } : undefined}
    > {/* E11: 弹期栏上浮至面板顶（面板恒在栏正下方——TA 16:19 裁决）;kbd/safe 基线保留 */}
      {btn('↶', '撤销', canUndo, () => runCmd(view, undo))}
      {btn('↷', '重做', canRedo, () => runCmd(view, redo))}
      <span className="tb-sep" />
      {/* 原子笔记对标 E1: ☰ 块组——平铺 6 钮收纳为弹面板（H1-H3/代码块/任务/表格+表内行列手术） */}
      <div className="tb-pop-wrap">
        {btn('☰', '块类型', BLOCK_BTNS.some((b) => blocks.has(b.key)), () => { if (pop !== 'block') hideKbdForPanel(); else restoreKbdAfterPanel(); setPop(pop === 'block' ? null : 'block'); },
          { fontSize: 15 })}
        {pop === 'block' && createPortal(
          <div ref={popRef} className="tb-style-pop" role="menu" aria-label="块类型面板">
            <div className="tb-sec">
            <div className="tb-sec-title">块类型</div>
            <div className="tb-style-grid-btns">
              {BLOCK_BTNS.map((b) => (
                <button key={b.key} className={`tb-style-btn ${blocks.has(b.key) ? 'hl-prev' : ''}`} title={b.title}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => { runCmd(view, b.cmd); }}>{b.label}</button>
              ))}
              {view && inTable(view.state) && TABLE_BTNS.map((b) => (
                <button key={b.title} className="tb-style-btn" title={b.title}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => { runCmd(view, b.cmd); }}>{b.label}</button>
              ))}
            </div>
            </div>
          </div>
        , document.body)}
      </div>
      {/* E5 整改: MARK_BTNS 平铺（B/I/U/S/代码）与 Aa「格式」tab 重复——平铺段删,全并入 Aa（TA 19:26 查重令） */}
      <div className="tb-pop-wrap">
        {btn('Aa', '文字格式', marks.has('fontColor') || marks.has('fontSize') || marks.has('highlight') || marks.has('strong') || marks.has('em') || marks.has('underline') || marks.has('strikethrough'),
          () => { if (!(pop === 'style' || pop === 'symbol' || pop === 'template')) hideKbdForPanel(); else restoreKbdAfterPanel(); setStyleTab('fmt'); setPop(pop === 'style' || pop === 'symbol' || pop === 'template' ? null : 'style'); },
          { fontSize: 13, fontWeight: 700 })}
        {(pop === 'style' || pop === 'symbol' || pop === 'template') && createPortal(
          <div ref={popRef} className="tb-style-pop" role="menu" aria-label="样式预设面板">
            <div className="tb-style-tabs" role="tablist">
              <button className={`tb-style-tab ${styleTab === 'fmt' ? 'on' : ''}`} role="tab" aria-selected={styleTab === 'fmt'}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => setStyleTab('fmt')}>格式</button>
              <button className={`tb-style-tab ${styleTab === 'style' ? 'on' : ''}`} role="tab" aria-selected={styleTab === 'style'}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => setStyleTab('style')}>文字样式</button>
              <button className={`tb-style-tab ${styleTab === 'symbol' ? 'on' : ''}`} role="tab" aria-selected={styleTab === 'symbol'}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => setStyleTab('symbol')}>符号</button>
              <button className={`tb-style-tab ${styleTab === 'template' ? 'on' : ''}`} role="tab" aria-selected={styleTab === 'template'}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => setStyleTab('template')}>图文模板</button>
            </div>
            {styleTab === 'fmt' ? (
              <div className="tb-fmt-panel">
                <div className="tb-sec">
                <div className="tb-fmt-row" role="group" aria-label="基础格式">
                  {btn('B', '加粗', marks.has('strong'), () => runCmd(view, toggleMark(auroraSchema.marks.strong)), { fontWeight: 800 })}
                  {btn('I', '斜体', marks.has('em'), () => runCmd(view, toggleMark(auroraSchema.marks.em)), { fontStyle: 'italic' })}
                  {btn('U', '下划线', marks.has('underline'), () => runCmd(view, toggleMark(auroraSchema.marks.underline)), { textDecoration: 'underline' })}
                  {btn('S', '删除线', marks.has('strikethrough'), () => runCmd(view, toggleMark(auroraSchema.marks.strikethrough)), { textDecoration: 'line-through' })}
                  {btn('</>', '行内代码', marks.has('code'), () => runCmd(view, toggleMark(auroraSchema.marks.code)), { fontFamily: 'monospace', fontSize: 12 })}
                </div>
                </div>
                <div className="tb-sec">
                <div className="tb-sec-title">字号</div>
                <div className="tb-pop" role="group" aria-label="字号档位">
                  {SIZES.map(([px, label]) => (
                    <button key={px} className="tb-size" title={`${px}px`}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => {
                        if (!view) return;
                        const { from, to } = view.state.selection;
                        view.dispatch(view.state.tr.removeMark(from, to, auroraSchema.marks.fontSize));
                        runCmd(view, toggleMark(auroraSchema.marks.fontSize, { size: px }));
                      }}>{label}</button>
                  ))}
                </div>
                </div>
                <div className="tb-sec">
                <div className="tb-sec-title">文字颜色</div>
                <div className="tb-pop" role="group" aria-label="文字色板">
                  <button className="tb-pop-reset" title="清除颜色"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => clearMark('fontColor')}>无</button>
                  {FONT_COLORS.map((c) => (
                    <button key={c} className="tb-swatch" title={`文字色 ${c}`} aria-label={`文字色 ${c}`}
                      style={{ background: c }}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => { runCmd(view, toggleMark(auroraSchema.marks.fontColor, { color: c })); }} />
                  ))}
                </div>
                </div>
                <div className="tb-sec">
                <div className="tb-sec-title">高亮</div>
                <div className="tb-pop" role="group" aria-label="高亮色板">
                  <button className="tb-pop-reset" title="清除高亮"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => clearMark('highlight')}>无</button>
                  {HL_COLORS.map((c) => (
                    <button key={c} className="tb-swatch" title={`高亮 ${c}`} aria-label={`高亮 ${c}`}
                      style={{ background: c }}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => { runCmd(view, toggleMark(auroraSchema.marks.highlight, { color: c })); }} />
                  ))}
                </div>
                </div>
              </div>
            ) : styleTab === 'symbol' ? (
              <div className="tb-symbol-grid">
                {['✓', '✗', '★', '☆', '♥', '♦', '●', '◆', '■', '▸', '▹', '▪', '☑', '☒', '✦', '※'].map((sym) => (
                  <button key={sym} className="tb-symbol" title={`插入 ${sym}`} aria-label={`插入符号 ${sym}`}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => {
                      if (!view) return;
                      const { from, to } = view.state.selection;
                      view.dispatch(view.state.tr.insertText(sym, from, to));
                     
                    }}>{sym}</button>
                ))}
              </div>
            ) : styleTab === 'template' ? (
              <div className="tb-tmpl-grid">
                {TEMPLATES.map((t) => (
                  <button key={t.key} className="tb-tmpl" title={t.title} aria-label={`模板 ${t.title}`}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => { insertTemplate(view, t.key); }}>
                    <span className="tb-tmpl-icon">{t.icon}</span>
                    <span className="tb-tmpl-name">{t.title}</span>
                  </button>
                ))}
              </div>
            ) : (
              <div className="tb-style-grid-btns">
            <button className="tb-style-btn" title="普通文本"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                if (!view) return;
                const { from, to } = view.state.selection;
                const tr = view.state.tr;
                ['fontColor', 'highlight', 'tagMark', 'quoteMark'].forEach((n) =>
                  tr.removeMark(from, to, auroraSchema.marks[n]));
                view.dispatch(tr);
               
              }}>普通</button>
            <button className="tb-style-btn blockquote-prev" title="引用块（红竖线）"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => { runCmd(view, wrapIn(auroraSchema.nodes.blockquote)); }}>引用</button>
            <button className="tb-style-btn tag-prev" title="标记（‹› 包裹+下划线）"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => { runCmd(view, toggleMark(auroraSchema.marks.tagMark)); }}>标记</button>
            <button className="tb-style-btn quote-prev" title="引语（“” 蓝色斜体）"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => { runCmd(view, toggleMark(auroraSchema.marks.quoteMark)); }}>引语</button>
            <button className="tb-style-btn hl-prev" title="高亮背景（黄）"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => { runCmd(view, toggleMark(auroraSchema.marks.highlight, { color: '#fef08a' })); }}>高亮</button>
            <button className="tb-style-btn red-prev" title="红色强调（红+粗）"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                if (!view) return;
                const { from, to } = view.state.selection;
                const tr = view.state.tr;
                tr.removeMark(from, to, auroraSchema.marks.fontColor);
                tr.addMark(from, to, auroraSchema.mark('fontColor', { color: '#dc2626' }));
                tr.addMark(from, to, auroraSchema.mark('strong'));
                view.dispatch(tr);
               
              }}>红强调</button>
              </div>
            )}
          </div>
        , document.body)}
      </div>
      {/* 原子笔记对标 E1: ＋ 插入组——列表/分割线/公式/流程图收纳为弹面板 */}
      <div className="tb-pop-wrap">
        {btn('＋', '插入', false, () => { if (pop !== 'insert') hideKbdForPanel(); else restoreKbdAfterPanel(); setPop(pop === 'insert' ? null : 'insert'); },
          { fontSize: 16 })}
        {pop === 'insert' && createPortal(
          <div ref={popRef} className="tb-style-pop" role="menu" aria-label="插入面板">
            <div className="tb-sec">
            <div className="tb-sec-title">插入</div>
            <div className="tb-style-grid-btns">
              {LIST_BTNS.map((b) => (
                <button key={b.key} className={`tb-style-btn ${blocks.has(b.key) ? 'hl-prev' : ''}`} title={b.title}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => { runCmd(view, b.cmd); }}>{b.label}</button>
              ))}
              <button className="tb-style-btn" title="分割线"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => { insertHorizontalRule(view); }}>—</button>
              <button className="tb-style-btn" title="插入公式"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => { if (view) insertMathBlock(view); }}>ƒx</button>
              <button className="tb-style-btn" title="插入流程图"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => { if (view) insertMermaidBlock(view); }}>❋</button>
              {/* E7 补齐（思源对标·TA 08:46）: 链接/图片/表格——「＋」插入组缺项 */}
              <button className="tb-style-btn" title="插入链接"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => { if (view) { const href = window.prompt('链接地址', 'https://'); if (href) { const { empty } = view.state.selection; if (!empty) { const linkMark = auroraSchema.marks.link; view.dispatch(view.state.tr.addStoredMark(linkMark.create({ href }))); } } } }}>🔗</button>
              <button className="tb-style-btn" title="插入图片（URL）"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => { if (view) { const url = window.prompt('图片地址', 'https://'); if (url) { const embed = auroraSchema.nodes.embed.create({ embed_type: 'image', url, title: '' }); const pos = view.state.selection.$from.before(1); view.dispatch(view.state.tr.insert(pos, embed)); } } }}>🖼</button>
              <button className="tb-style-btn" title="插入表格"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => { if (view) insertBlock(view, 'table'); }}>▦</button>
            </div>
            </div>
          </div>
        , document.body)}
      </div>
    </div>
    </>,
    document.body,
  );
}

// ---------------------------------------------------------------------------
// 主组件
// ---------------------------------------------------------------------------

export function RichEditor({ noteId, platform, fallbackText, onDirty, onSaved, onStatus, mobileDock }: RichEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const handleRef = useRef<AuroraEditorHandle | null>(null);
  const dirtyRef = useRef(false);
  const [status, setStatus] = useState<EditorStatus>('loading');
  const [tick, setTick] = useState(0);
  const [fallbackContent, setFallbackContent] = useState(fallbackText);

  const changeStatus = (s: EditorStatus) => {
    setStatus(s);
    onStatus?.(s);
  };

  useEffect(() => {
    let cancelled = false;
    dirtyRef.current = false;
    changeStatus('loading');

    // 无桥 → 降级
    const snapB64 = platform.getNoteSnapshot(noteId);
    if (snapB64 === null) {
      changeStatus('fallback');
      return;
    }

    // 有桥：初始化 Loro + ProseMirror
    try {
      const doc = loroDocFromBase64(snapB64);
      if (!hostRef.current || cancelled) {
        doc.free?.();
        return;
      }

      const handle = createAuroraEditor(hostRef.current, {
        loroDoc: doc,
        onSave: (snapshotBytes) => {
          // Uint8Array → base64 → JNI（CRDT 合并到 Rust 侧 NoteDoc）
          const b64 = bytesToBase64(snapshotBytes);
          if (!platform.saveNoteSnapshot(noteId, b64)) {
            console.warn('saveNoteSnapshot failed for', noteId);
          } else {
            onSaved?.();
          }
          dirtyRef.current = false;
        },
        onUpdate: () => {
          // DK-05M: 输入法组合期间跳过 React 重渲染 — 重渲染会重建
          // DOM 使光标跳动（DoD「候选期间光标不跳动」）; 组合结束后的
          // 最后一次事务会自然触发更新
          const h = handleRef.current;
          if (h && h.view.composing) return;
          setTick((t) => t + 1);
        },
      });
      if (cancelled) {
        handle.destroy();
        return;
      }
      handleRef.current = handle;
      // E2 测试通道: PW 直驱 PM selection（DOM Range 被 PM 失焦重建吞——window.__editorView）
      (window as unknown as Record<string, unknown>).__editorView = handle.view;
      changeStatus('rich');
    } catch (e) {
      console.error('editor init failed, falling back', e);
      if (!cancelled) changeStatus('fallback');
    }

    return () => {
      cancelled = true;
      const h = handleRef.current;
      if (h) {
        if (dirtyRef.current) h.flushSave();
        h.destroy();
        handleRef.current = null;
        delete (window as unknown as Record<string, unknown>).__editorView;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noteId]);

  // 降级 textarea
  if (status === 'fallback') {
    const ta = taRef.current;
    /** 统一文本操作入口: setState + 落盘 + rAF 恢复光标。 */
    const applyOp = (op: TextOp) => {
      setFallbackContent(op.text);
      onDirty?.();
      platform.saveNoteContent(noteId, op.text);
      requestAnimationFrame(() => {
        const el = taRef.current;
        if (el) {
          el.focus();
          el.setSelectionRange(op.caret, op.caret);
        }
      });
    };
    const lineOp = (prefix: string | null) => {
      if (!ta) return;
      applyOp(toggleLinePrefix(ta.value, ta.selectionStart, prefix));
    };
    const wrapOp = (mark: string) => {
      if (!ta) return;
      applyOp(wrapSelection(ta.value, ta.selectionStart, ta.selectionEnd, mark));
    };
    const fbBtn = (label: string, title: string, onClick: () => void) => (
      <button key={title} className="tb-btn" title={title} onMouseDown={(e) => e.preventDefault()} onClick={onClick}>
        {label}
      </button>
    );
    return (
      <div className="editor-fallback-wrap" style={{ flexDirection: 'column' }}>
        <textarea
          ref={taRef}
          className="editor-textarea"
          value={fallbackContent}
          onChange={(e) => {
            setFallbackContent(e.target.value);
            onDirty?.();
            platform.saveNoteContent(noteId, e.target.value);
          }}
          placeholder="开始写作…（降级模式：纯文本）"
        />
        <div className="editor-toolbar" role="toolbar" aria-label="Markdown 工具栏（降级模式）">
          {fbBtn('H1', '标题 1', () => lineOp('# '))}
          {fbBtn('H2', '标题 2', () => lineOp('## '))}
          {fbBtn('H3', '标题 3', () => lineOp('### '))}
          {fbBtn('正文', '回正文', () => lineOp(null))}
          <span className="tb-sep" />
          {fbBtn('B', '加粗', () => wrapOp('**'))}
          {fbBtn('I', '斜体', () => wrapOp('*'))}
          {fbBtn('</>', '行内代码', () => wrapOp('`'))}
          <span className="tb-sep" />
          {fbBtn('•≡', '无序列表', () => lineOp('- '))}
          {fbBtn('☑', '任务', () => lineOp('- [ ] '))}
          {fbBtn('❝', '引用', () => lineOp('> '))}
          <span className="tb-sep" />
          {fbBtn('—', '分割线', () => {
            if (!ta) return;
            const s = ta.selectionStart;
            applyOp({ text: ta.value.slice(0, s) + '\n---\n' + ta.value.slice(s), caret: s + 5 });
          })}
        </div>
      </div>
    );
  }

  return (
    <div className={`rich-editor-wrap ${status === 'loading' ? 'loading' : ''}`}>
      <div className="rich-editor-host" ref={hostRef} />
      {status === 'rich' && (
        <>
          <FloatingMenu view={handleRef.current?.view ?? null} />
          <EditorToolbar view={handleRef.current?.view ?? null} tick={tick} />
          {mobileDock && <MobileEditorDock view={handleRef.current?.view ?? null} tick={tick} />}
        </>
      )}
      {status === 'loading' && (
        <div className="editor-loading-tip">
          <span className="spinner" />
          编辑器加载中…
        </div>
      )}
    </div>
  );
}
