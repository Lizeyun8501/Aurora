/**
 * DK-2.7 切片七：公式 inputRules + 插入命令单测。
 * 覆盖：$..$ 闭 $ 即转 math_inline / 货币不触发 / 插入命令节点类型。
 * inputRules 插件必须进 EditorState.create（带 state 组件不能直传 view）。
 */
import { describe, expect, it } from 'vitest';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { inputRules } from 'prosemirror-inputrules';
import { mathInlineRule, mathBlockRule, insertMathBlock, insertMermaidBlock } from '../editors/mathInputRules';
import { auroraSchema } from '../schema/auroraSchema';

function typeText(view: EditorView, text: string) {
  // 模拟真实 DOM 输入：inputRule 走 handleTextInput（insertText 不触发）
  for (const ch of text) {
    const pos = view.state.selection.from;
    view.someProp('handleTextInput', (f) =>
      f(view, pos, pos, ch, () => view.state.tr.insertText(ch)),
    );
    view.dispatch(view.state.tr.insertText(ch));
  }
}

describe('DK-2.7 切片七：公式 inputRules', () => {
  it('$E=mc^2$ 闭 $ 即转 math_inline', () => {
    const state = EditorState.create({
      doc: auroraSchema.nodes.doc.create(null, [
        auroraSchema.nodes.paragraph.create(null, null),
      ]),
      schema: auroraSchema,
      plugins: [inputRules({ rules: [mathInlineRule, mathBlockRule] })],
    });
    const view = new EditorView(document.createElement('div'), { state });
    for (const ch of '$E=mc^2$') typeText(view, ch);
    const inline = view.state.doc.nodeAt(1);
    expect(inline?.type.name).toBe('math_inline');
    expect(inline?.attrs.latex).toBe('E=mc^2');
    view.destroy();
  });

  it('货币 "$5 and $6" 不触发（闭 $ 前空白启发式）', () => {
    const state = EditorState.create({
      doc: auroraSchema.nodes.doc.create(null, [
        auroraSchema.nodes.paragraph.create(null, null),
      ]),
      schema: auroraSchema,
      plugins: [inputRules({ rules: [mathInlineRule] })],
    });
    const view = new EditorView(document.createElement('div'), { state });
    for (const ch of '$5 and $6') typeText(view, ch);
    const inline = view.state.doc.nodeAt(1);
    expect(inline?.type.name).not.toBe('math_inline');
    view.destroy();
  });
});

describe('DK-2.7 切片七：插入命令', () => {
  it('insertMathBlock 落 math_block 节点', () => {
    const state = EditorState.create({
      doc: auroraSchema.nodes.doc.create(null, [
        auroraSchema.nodes.paragraph.create(null, auroraSchema.text('前文')),
      ]),
      schema: auroraSchema,
    });
    const view = new EditorView(document.createElement('div'), { state });
    view.dispatch(
      view.state.tr.setSelection(TextSelection.near(view.state.doc.resolve(1))),
    );
    expect(insertMathBlock(view)).toBe(true);
    const names = view.state.doc.content.content.map((n) => n.type.name);
    expect(names).toContain('math_block');
    view.destroy();
  });

  it('insertMermaidBlock 落 mermaid code_block', () => {
    const state = EditorState.create({
      doc: auroraSchema.nodes.doc.create(null, [
        auroraSchema.nodes.paragraph.create(null, auroraSchema.text('前文')),
      ]),
      schema: auroraSchema,
    });
    const view = new EditorView(document.createElement('div'), { state });
    view.dispatch(
      view.state.tr.setSelection(TextSelection.near(view.state.doc.resolve(1))),
    );
    expect(insertMermaidBlock(view)).toBe(true);
    const block = view.state.doc.content.content.find(
      (n) => n.type.name === 'code_block',
    );
    expect(block?.attrs.language).toBe('mermaid');
    view.destroy();
  });
});
