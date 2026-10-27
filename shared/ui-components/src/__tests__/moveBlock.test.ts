/**
 * R19-⑤b 深挖单测：moveBlock 顶层 swap 数学（node 环境重建 EditorState 直测）
 *
 * 对向复核现场（lab Playwright [OL,BQ] 上移后变 [OL,P]——BQ 消失）：
 * 排除浏览器层（PM click 补段/LoroUndoPlugin），锁定 tr.delete+insert 数学。
 */
import { describe, it, expect } from 'vitest';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { auroraSchema as schema } from '../schema/auroraSchema';
import { moveBlock } from '../editors/insertCommands';

/** 顶层块标签序列。 */
function tags(state: EditorState): string[] {
  const out: string[] = [];
  state.doc.forEach((n) => out.push(n.type.name === 'bullet_list' ? 'OL' : n.type.name === 'ordered_list' ? 'OL' : n.type.name.toUpperCase()));
  return out;
}

function mkView(doc: ReturnType<typeof schema.nodes.doc.create>): EditorView {
  const state = EditorState.create({ doc, schema });
  // node 环境 EditorView 需要挂载点（ vitest jsdom 环境 document 可用）
  const host = document.createElement('div');
  document.body.appendChild(host);
  return new EditorView(host, { state });
}

describe('R19-⑤b moveBlock 顶层 swap', () => {
  it('[UL, BLOCKQUOTE] 上移 BQ → [BLOCKQUOTE, UL]（BQ 不消失）', () => {
    const p = schema.nodes.paragraph.create(null, schema.text('x'));
    const ul = schema.nodes.bullet_list.create(null, [schema.nodes.list_item.create(null, p)]);
    const bqP = schema.nodes.paragraph.create(null, schema.text('q'));
    const bq = schema.nodes.blockquote.create(null, [bqP]);
    const doc = schema.nodes.doc.create(null, [ul, bq]);
    const view = mkView(doc);
    // 光标置 BQ 内
    const inBq = view.state.doc.resolve(view.state.doc.content.size - 2);
    view.dispatch(view.state.tr.setSelection(TextSelection.near(inBq)));
    const before = tags(view.state);
    expect(before).toEqual(['OL', 'BLOCKQUOTE']);
    const ok = moveBlock(view, -1);
    expect(ok).toBe(true);
    const after = tags(view.state);
    expect(after).toEqual(['BLOCKQUOTE', 'OL']);
    view.destroy();
  });

  it('[UL, 空 BLOCKQUOTE] 上移空 BQ → 空块保留（查 PM 是否有空块换 P 补段路径）', () => {
    const p = schema.nodes.paragraph.create(null, schema.text('x'));
    const ul = schema.nodes.bullet_list.create(null, [schema.nodes.list_item.create(null, p)]);
    const bqEmptyP = schema.nodes.paragraph.create(null);
    const bq = schema.nodes.blockquote.create(null, [bqEmptyP]);
    const doc = schema.nodes.doc.create(null, [ul, bq]);
    const view = mkView(doc);
    const inBq = view.state.doc.resolve(view.state.doc.content.size - 2);
    view.dispatch(view.state.tr.setSelection(TextSelection.near(inBq)));
    const ok = moveBlock(view, -1);
    expect(ok).toBe(true);
    const after = tags(view.state);
    // 期望空 BQ 保留；若 PM 有自动补段/空块替换路径会在此暴露
    expect(after).toEqual(['BLOCKQUOTE', 'OL']);
    view.destroy();
  });

  it('[H1, H2, P] 末块连续下移禁用（边界守卫）', () => {
    const h1 = schema.nodes.heading.create({ level: 1 }, schema.text('a'));
    const h2 = schema.nodes.heading.create({ level: 2 }, schema.text('b'));
    const p = schema.nodes.paragraph.create(null, schema.text('c'));
    const doc = schema.nodes.doc.create(null, [h1, h2, p]);
    const view = mkView(doc);
    const inP = view.state.doc.resolve(view.state.doc.content.size - 1);
    view.dispatch(view.state.tr.setSelection(TextSelection.near(inP)));
    expect(moveBlock(view, 1)).toBe(false);
    view.destroy();
  });
});
