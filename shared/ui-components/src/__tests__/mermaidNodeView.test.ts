/**
 * DK-2.7 切片六：mermaid NodeView 逻辑单测——非渲染面（mermaid 库
 * 需要 SVG 布局 API，jsdom 不具备；渲染真值手动验收）。
 * 走完整 EditorView+nodeViews 流程（PM 真实认领 NodeView）。
 */
import { describe, expect, it } from 'vitest';
import { EditorState } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { mermaidNodeViews } from '../editors/mermaidNodeView';
import { auroraSchema } from '../schema/auroraSchema';

function codeBlock(lang: string, text: string) {
  return auroraSchema.nodes.code_block.create(
    { language: lang },
    auroraSchema.text(text),
  );
}

function mkView(lang: string, text: string, dispatchSpy?: unknown[]) {
  const state = EditorState.create({
    doc: auroraSchema.nodes.doc.create(null, [codeBlock(lang, text)]),
    schema: auroraSchema,
  });
  const dom = document.createElement('div');
  document.body.appendChild(dom);
  const view = new EditorView(dom, {
    state,
    nodeViews: mermaidNodeViews(),
    dispatchTransaction(tx) {
      if (dispatchSpy) dispatchSpy.push(tx);
      view.updateState(view.state.apply(tx));
    },
  });
  return view;
}

describe('DK-2.7 切片六：mermaid NodeView（逻辑面）', () => {
  it('普通 code_block：默认编辑面——contentDOM 承接源码可编辑', () => {
    const view = mkView('rust', 'fn main() {}');
    const pre = view.dom.querySelector('pre')!;
    expect(pre.querySelector('code')).toBeTruthy();
    // 非 mermaid：无渲染容器类名（不接管渲染）
    expect(pre.className).not.toContain('aurora-mermaid');
    view.destroy();
  });

  it('language 切换 mermaid→rust：NodeView 重建为默认面', () => {
    const view = mkView('mermaid', 'graph TD; A-->B;');
    expect(view.dom.querySelector('.aurora-mermaid')).toBeTruthy();
    // 切语言（EditBar/工具条真实路径同款 setNodeAttribute）
    const tr = view.state.tr.setNodeAttribute(0, 'language', 'rust');
    view.dispatch(tr);
    // 重建后：默认面接管（无渲染容器）
    expect(view.dom.querySelector('.aurora-mermaid')).toBeNull();
    view.destroy();
  });

  it('language 切换 rust→mermaid：重建为渲染容器（jsdom 下渲染失败回退显源码）', () => {
    const view = mkView('rust', 'flowchart TD');
    expect(view.dom.querySelector('.aurora-mermaid')).toBeNull();
    const tr = view.state.tr.setNodeAttribute(0, 'language', 'mermaid');
    view.dispatch(tr);
    // mermaid import 在 jsdom 异步失败——回退面仍应建立（渲染容器或源码回显）
    expect(view.dom.querySelector('.aurora-mermaid')).toBeTruthy();
    view.destroy();
  });

  it('mermaid 内容变更（协同回写路径）：渲染容器保持存在', () => {
    const view = mkView('mermaid', 'graph TD; A-->B;');
    const tr = view.state.tr.insertText('x', 1);
    view.dispatch(tr);
    expect(view.dom.querySelector('.aurora-mermaid')).toBeTruthy();
    view.destroy();
  });
});
