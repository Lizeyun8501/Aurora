/**
 * Markdown 输入规则（E8 · 优质软件调研——Bear/思源核心体验）
 * 「# 」→标题 /「> 」→引用 /「- 」「* 」「+ 」→无序列表 /「1. 」→有序列表 /
 * 「[] 」「[x] 」→任务 /「```lang 」→代码块 /「---」→分割线 /
 * 内联: 双星粗/单星斜/双波浪删/反引号码/双等号高亮
 * 依据: prosemirror-example-setup markdown 惯例 + Aurora schema（task_block 语义差异适配）
 */
import { InputRule, inputRules, wrappingInputRule, textblockTypeInputRule } from 'prosemirror-inputrules';
import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorState as PMEditorState, Transaction } from 'prosemirror-state';

/** shape 声明（PM 类型面较严——运行时 PM 全有） */
interface ResolvedCursor {
  pos: number;
  parentOffset: number;
  parent: {
    type: { spec: { code?: boolean } };
    textBetween: (a: number, b: number, c: null, d: string) => string;
  };
}
interface RuleInternals {
  match: RegExp;
  inCodeMark?: boolean | 'only';
  handler: (st: PMEditorState, match: RegExpMatchArray, from: number, to: number) => Transaction | null;
}
import type { EditorState } from 'prosemirror-state';
import type { MarkType, NodeType } from 'prosemirror-model';
import { auroraSchema } from '../schema/auroraSchema';

/** 内联 mark 规则——匹配整段即替换为带 mark 文本;inner=捕获组序（单组 regex=1） */
function inlineMarkRule(mark: MarkType, rx: RegExp, inner: number): InputRule {
  return new InputRule(rx, (state: EditorState, match: RegExpMatchArray, from: number, to: number): Transaction => {
    const tr = state.tr;
    const text = match[inner] ?? '';
    tr.delete(from, to);
    const start = tr.selection.from;
    tr.insertText(text);
    tr.addMark(start, start + text.length, mark.create());
    return tr;
  });
}

/** 标题: 「# 」..「###### 」→ heading level 1-6 */
function headingRule(node: NodeType): InputRule {
  return textblockTypeInputRule(/^#{1,6} $/, node, (m: RegExpMatchArray) => ({ level: m[0].length - 1 }));
}

/** 代码块: 「```lang 」→ code_block（空语言退化 plaintext） */
function codeRule(node: NodeType): InputRule {
  return new InputRule(/^```(\w*) $/, (state: EditorState, m: RegExpMatchArray, from: number, to: number): Transaction => {
    const tr = state.tr;
    tr.delete(from, to);
    const $pos = tr.doc.resolve(tr.selection.from);
    const lang = m[1] || 'plaintext';
    if ($pos.parent.type.name === 'paragraph') {
      const pStart = tr.selection.from - 4 - lang.length; // 「```lang 」已删——整段替换
      const pEnd = $pos.parentOffset === 0 ? pStart + $pos.parent.nodeSize : tr.selection.from;
      tr.replaceWith(Math.max(pStart, 0), pEnd, node.create({ language: lang }));
    }
    return tr;
  });
}

/** 任务块: 「[ ] 」「[x] 」→ task_block（整段转任务,余文保留——Bear 语义） */
function taskRule(node: NodeType): InputRule[] {
  const mk = (checked: boolean) => new InputRule(/^\[( |x|X)\] $/, (state: EditorState, _m: RegExpMatchArray, from: number, to: number): Transaction => {
    const tr = state.tr;
    const $pos = tr.doc.resolve(from);
    if ($pos.parent.type.name !== 'paragraph') return tr.delete(from, to);
    const pStart = $pos.before(1); // 段绝对 start（open 前——PM 权威 API;from-parentOffset 在 content 起点处偏 +1 会越界
    // 余文: 段 content 中语法后的部分（相对 cut
    const rel = to - (pStart + 1);
    const frag = $pos.parent.cut(rel, $pos.parent.content.size).content;
    const task = node.create({ checked, task_id: null }, frag.size ? frag : undefined);
    tr.replaceWith(pStart, pStart + $pos.parent.nodeSize, task);
    return tr;
  });
  return [mk(false), mk(true)];
}

/**
 * E8 移动端增强: Space 键 keydown 先行触发（合成前）
 * 根因: 移动 IME/headless 合成 insertText 绕 handleTextInput（prosemirror#1096）
 * ——keydown 在合成前到达,命中即 dispatch+preventDefault（语法已吃,空格不须再插）;
 * 未命中放行原生输入（桌面 hti 链仍双保险,规则幂等: 引擎 hti 若再跑,匹配已消）。
 */
function mobileSpaceKeyPlugin(rules: InputRule[]): Plugin {
  return new Plugin({
    key: new PluginKey('e8MarkdownSpace'),
    props: {
      handleKeyDown(view, e) {
        if (e.key !== ' ') return false;
        const { state } = view;
        if (view.composing) return false;
        const $cursor = (state.selection as unknown as { $cursor?: ResolvedCursor }).$cursor;
        if (!$cursor || $cursor.parent.type.spec.code) return false;
        const textBefore = $cursor.parent.textBetween(0, $cursor.parentOffset, null, '\uFFFC') + ' ';
        for (const rule of rules) {
          const m = (rule as unknown as RuleInternals).match.exec(textBefore);
          if (!m || m[0].length < 1) continue;
          const startPos = $cursor.pos - (m[0].length - 1);
          const end = $cursor.pos + 1;
          // code mark 语境跳过（与引擎 run 同判）
          if (rule.inCodeMark == null) {
            let hasCodeMark = false;
            state.doc.nodesBetween(startPos, $cursor.pos, (n: { isInline: boolean; marks: { some: (f: (mk: { type: { spec: { code?: boolean } } }) => boolean) => boolean } }) => {
              if (n.isInline && n.marks.some((mk) => !!mk.type.spec.code)) hasCodeMark = true;
              return !hasCodeMark;
            });
            if (hasCodeMark) continue;
          }
          const tr = (rule as unknown as RuleInternals).handler(
            state, m as unknown as RegExpMatchArray, startPos, end,
          ) as import('prosemirror-state').Transaction | null;
          if (tr) {
            view.dispatch(tr.scrollIntoView());
            view.focus();
            e.preventDefault();
            return true;
          }
        }
        return false;
      },
    },
  });
}

/** E8: markdown 快捷输入全家桶（Bear/思源系标准体验） */
export function markdownInputRules(): Plugin[] {
  const { nodes, marks } = auroraSchema;
  const rules: InputRule[] = [
    headingRule(nodes.heading),
    codeRule(nodes.code_block),
    ...taskRule(nodes.task_block),
    // 引用/列表: wrappingInputRule 官方惯例
    wrappingInputRule(/^\s*>\s$/, nodes.blockquote),
    wrappingInputRule(/^\s*[-*+]\s$/, nodes.bullet_list),
    wrappingInputRule(/^\s*(1)[.)]\s$/, nodes.ordered_list),
    // 分割线: --- / ***（段首语境）
    new InputRule(/^(---|\*\*\*)$/, (state: EditorState, _m: RegExpMatchArray, from: number, to: number): Transaction => {
      const tr = state.tr;
      const $pos = tr.doc.resolve(from);
      if ($pos.parent.type.name === 'paragraph' && $pos.parentOffset === 0) {
        tr.replaceWith(from, to, nodes.horizontal_rule.create());
      } else tr.delete(from, to);
      return tr;
    }),
    // 内联 marks（行尾续写即触发;单组捕获 regex inner=1）
    inlineMarkRule(marks.strong, /(\*\*|__)([^*_]+)(\*\*|__)$/, 2),
    inlineMarkRule(marks.em, /(?:^|[^*_])\*([^*_]+)\*$/, 1), // E8 修: 首星前须非星——防双星中途劫持（example-setup 惯例）
    inlineMarkRule(marks.strikethrough, /~~([^~]+)~~$/, 1),
    inlineMarkRule(marks.code, /`([^`]+)`$/, 1),
    inlineMarkRule(marks.highlight, /==([^=]+)==$/, 1),
  ];
  return [inputRules({ rules }), mobileSpaceKeyPlugin(rules)];
}
