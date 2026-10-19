/**
 * DK-2.7 切片五：公式 NodeView — KaTeX 渲染态 + 聚焦源码编辑态。
 *
 * Typora 式交互：非聚焦渲染 KaTeX（atom 节点 contenteditable=false）；
 * 选中/双击进入编辑（textarea 覆盖层），blur commit attrs、Esc 取消。
 * KaTeX 懒加载（首次渲染才 import——桌面首载不膨胀）。
 * Loro 同步只看 attrs.latex——NodeView 是纯视图面。
 */
import { Node as ProseNode } from 'prosemirror-model';
import { EditorView, NodeView } from 'prosemirror-view';

let katexPromise: Promise<typeof import('katex')> | null = null;
function loadKatex() {
  katexPromise ??= import('katex');
  return katexPromise;
}

export class MathNodeView implements NodeView {
  dom: HTMLElement;
  private node: ProseNode;
  private view: EditorView;
  private getPos: () => number | undefined;
  private renderEl: HTMLElement;
  private editEl: HTMLTextAreaElement | null = null;
  private editing = false;

  constructor(
    node: ProseNode,
    view: EditorView,
    getPos: () => number | undefined,
    block: boolean,
  ) {
    this.node = node;
    this.view = view;
    this.getPos = getPos;
    this.dom = document.createElement(block ? 'div' : 'span');
    this.dom.className = block ? 'aurora-math-block' : 'aurora-math-inline';
    this.dom.contentEditable = 'false';
    this.renderEl = document.createElement(block ? 'div' : 'span');
    this.renderEl.className = 'aurora-math-render';
    this.dom.appendChild(this.renderEl);
    // 渲染态点击=选中 atom（PM 原生行为保留——不 stopEvent）
    this.renderEl.addEventListener('dblclick', () => this.enterEdit());
    this.renderEl.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && this.view.state.selection.from === this.getPos()) {
        e.preventDefault();
        this.enterEdit();
      }
    });
    this.render(block);
  }

  /** KaTeX 渲染（懒加载；失败回退显源码——诚实降级不吞内容） */
  private async render(_block: boolean) {
    const latex = String(this.node.attrs.latex ?? '');
    try {
      const katex = await loadKatex();
      this.renderEl.innerHTML = '';
      const span = document.createElement('span');
      katex.render(latex, span, {
        throwOnError: false,
        displayMode: this.node.type.name === 'math_block',
      });
      this.renderEl.appendChild(span);
    } catch {
      this.renderEl.textContent = latex || '(空公式)';
    }
  }

  /** 进入编辑：textarea 覆盖渲染层（stopEvent 隔离 PM 键盘） */
  private enterEdit() {
    if (this.editing) return;
    this.editing = true;
    this.editEl = document.createElement('textarea');
    this.editEl.className = 'aurora-math-edit';
    this.editEl.value = String(this.node.attrs.latex ?? '');
    this.editEl.rows = this.node.type.name === 'math_block' ? 4 : 1;
    this.editEl.addEventListener('blur', () => this.commit());
    this.editEl.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); this.cancel(); }
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); this.commit(); }
      e.stopPropagation();
    });
    this.dom.appendChild(this.editEl);
    this.renderEl.style.display = 'none';
    this.editEl.focus();
  }

  private commit() {
    if (!this.editing || !this.editEl) return;
    const pos = this.getPos();
    if (pos === undefined) { this.cancel(); return; }
    const latex = this.editEl.value;
    this.editing = false;
    this.editEl.remove();
    this.editEl = null;
    this.renderEl.style.display = '';
    this.view.dispatch(
      this.view.state.tr.setNodeMarkup(pos, undefined, { latex }),
    );
  }

  private cancel() {
    this.editing = false;
    this.editEl?.remove();
    this.editEl = null;
    this.renderEl.style.display = '';
  }

  /** attrs 变更（协同/Loro 回写）重渲染 */
  update(node: ProseNode): boolean {
    if (node.type.name !== this.node.type.name) return false;
    const latexChanged = node.attrs.latex !== this.node.attrs.latex;
    this.node = node;
    if (latexChanged && !this.editing) this.render(node.type.name === 'math_block');
    return true;
  }

  ignoreMutation(): boolean {
    // 纯视图面——PM 的 DOM mutation 全忽略（重渲染由 update 驱动）
    return !this.editing;
  }

  stopEvent(_e: Event): boolean {
    // 编辑态事件全吃（textarea 内击键不给 PM）；渲染态放行（选中/拖拽）
    return this.editing;
  }

  destroy() {
    this.cancel();
  }
}

/** nodeViews 装配项（auroraEditor 挂载用） */
export function mathNodeViews(): Record<string, (node: ProseNode, view: EditorView, getPos: () => number | undefined) => NodeView> {
  return {
    math_inline: (node, view, getPos) =>
      new MathNodeView(node, view, getPos, false),
    math_block: (node, view, getPos) =>
      new MathNodeView(node, view, getPos, true),
  };
}
