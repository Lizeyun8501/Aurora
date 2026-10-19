/**
 * DK-2.7 切片七：公式输入规则 + 插入命令 — WYSIWYG 闭环。
 *
 * - mathInlineRule：输入 `$latex$`（闭 $）即刻替换为 math_inline 节点
 *   （Typora 式 live 转换；货币 "$5 and $6" 因闭 $ 前空白不触发）。
 * - mathBlockRule：行首输入 `$$`+空格 → math_block 节点。
 * - insertMathBlock / insertMermaidBlock：工具条插入入口（光标定位）。
 *
 * 与 mdToNodes 同一启发式：latex 内容首尾非空白才转换。
 */
import { EditorState, TextSelection, Transaction } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { InputRule, inputRules } from 'prosemirror-inputrules';

/** 行内公式：`$...$` 输入完成（闭 $ 触发）——内容首尾非空白 + 非 \$ 转义 */
export const mathInlineRule = new InputRule(
  /\$(?:\\\$|[^\s$]|\S[^$\n]*[^\s$])\$$/,
  (state: EditorState, match: RegExpMatchArray, start: number, end: number): Transaction | null => {
    const raw = match[0]; // 含首尾 $
    const latex = raw.slice(1, -1);
    if (!latex.trim() || /^\s|\s$/.test(latex)) return null; // 启发式同 mdToNodes
    const tr = state.tr;
    // 替换 $latex$ 区间为 math_inline（latex 存 attrs——内容原子）
    tr.replaceWith(
      start,
      end,
      state.schema.nodes.math_inline.create({ latex }),
    );
    // 光标落到公式后
    tr.setSelection(TextSelection.near(tr.doc.resolve(start + 1), 1));
    return tr;
  },
);

/** 块级公式：行首 `$$`+空格 → math_block（latex 留空——NodeView 进编辑态） */
export const mathBlockRule = new InputRule(
  /^\$\$$/,
  (state: EditorState, _match: RegExpMatchArray, start: number, end: number): Transaction | null => {
    const tr = state.tr;
    tr.replaceWith(
      start,
      end,
      state.schema.nodes.math_block.create({ latex: '' }),
    );
    tr.setSelection(TextSelection.near(tr.doc.resolve(start + 1), 1));
    return tr;
  },
);

/** 公式输入规则组（auroraEditor 插件栈挂载） */
export function mathInputRules() {
  return inputRules({ rules: [mathInlineRule, mathBlockRule] });
}

/**
 * 工具条插入：块级公式（当前块前插新 math_block，光标进 NodeView 编辑态
 * 由 dblclick/Enter 约定——v1 插入后聚焦节点）。
 */
export function insertMathBlock(view: EditorView): boolean {
  const { state } = view;
  const { $from } = state.selection;
  const pos = $from.before($from.depth);
  const tr = state.tr.insert(
    pos,
    state.schema.nodes.math_block.create({ latex: '' }),
  );
  view.dispatch(tr);
  return true;
}

/** 工具条插入：mermaid 块（code_block language=mermaid 空源码） */
export function insertMermaidBlock(view: EditorView): boolean {
  const { state } = view;
  const { $from } = state.selection;
  const pos = $from.before($from.depth);
  const block = state.schema.nodes.code_block.create(
    { language: 'mermaid' },
    state.schema.text('graph TD\n  A --> B'),
  );
  const tr = state.tr.insert(pos, block);
  view.dispatch(tr);
  return true;
}
