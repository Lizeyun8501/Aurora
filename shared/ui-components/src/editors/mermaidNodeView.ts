/**
 * DK-2.7 切片六：mermaid NodeView — code_block language=mermaid 的
 * 渲染态（SVG 图）+ 编辑态（textarea 源码），交互模式同公式切片五。
 *
 * 关键机制：
 * - nodeViews 按「节点类型」注册而非按 attrs——本 NodeView 接管全部
 *   code_block：language=mermaid 走渲染/编辑态；其余语言返回 contentDOM
 *   走 PM 默认编辑面（视觉零差异）。
 * - language 切换（mermaid↔其他）时 update 返回 false 触发 NodeView 重建。
 * - mermaid 懒 import（首次渲染才加载——桌面首载不膨胀）。
 * - 渲染失败回退显源码（诚实降级——同公式切片）。
 * - Loro 同步透明：code_block 内容即 textContent，NodeView 纯视图面。
 */
import { Node as ProseNode } from 'prosemirror-model';
import { EditorView, NodeView } from 'prosemirror-view';

let mermaidPromise: Promise<typeof import('mermaid').default> | null = null;
function loadMermaid() {
  mermaidPromise ??= import('mermaid').then((m) => {
    m.default.initialize({ startOnLoad: false, securityLevel: 'strict' });
    return m.default;
  });
  return mermaidPromise;
}

let renderSeq = 0;

export class MermaidNodeView implements NodeView {
  dom: HTMLElement;
  /** 非 mermaid 语言时提供——PM 走默认内容编辑面 */
  contentDOM?: HTMLElement;
  private node: ProseNode;
  private view: EditorView;
  private getPos: () => number | undefined;
  private renderEl: HTMLElement;
  private editEl: HTMLTextAreaElement | null = null;
  private editing = false;
  private destroyed = false;

  constructor(node: ProseNode, view: EditorView, getPos: () => number | undefined) {
    this.node = node;
    this.view = view;
    this.getPos = getPos;

    if (this.lang() !== 'mermaid') {
      // 非 mermaid：默认面——dom 由 PM 提供 wrapper，contentDOM 承接内容
      this.dom = document.createElement('pre');
      this.contentDOM = document.createElement('code');
      this.dom.appendChild(this.contentDOM);
      this.renderEl = this.dom; // 占位引用（update/destroy 不触碰）
      return;
    }

    this.dom = document.createElement('pre');
    this.dom.className = 'aurora-mermaid';
    this.dom.contentEditable = 'false';
    this.renderEl = document.createElement('div');
    this.renderEl.className = 'aurora-mermaid-render';
    this.dom.appendChild(this.renderEl);
    this.renderEl.addEventListener('dblclick', () => this.enterEdit());
    this.renderEl.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && this.view.state.selection.from === this.getPos()) {
        e.preventDefault();
        this.enterEdit();
      }
    });
    this.render();
  }

  private lang(): string {
    return String(this.node.attrs.language ?? 'plaintext');
  }

  private async render(): Promise<void> {
    const code = this.node.textContent;
    this.renderEl.textContent = code; // 先显源码占位（加载期/失败回退同款）
    try {
      const mermaid = await loadMermaid();
      if (this.destroyed) return;
      const { svg } = await mermaid.render(`aurora-mermaid-${++renderSeq}`, code);
      if (this.destroyed || this.editing) return;
      this.renderEl.innerHTML = svg; // securityLevel:strict 已消毒
    } catch {
      // 解析失败：保持源码显示（诚实降级——不吞内容）
    }
  }

  private enterEdit(): void {
    if (this.editing) return;
    this.editing = true;
    this.renderEl.style.display = 'none';
    const ta = document.createElement('textarea');
    ta.className = 'aurora-mermaid-edit';
    ta.value = this.node.textContent;
    ta.rows = Math.max(4, ta.value.split('\n').length);
    ta.addEventListener('blur', () => this.commit());
    ta.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); this.cancel(); }
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); this.commit(); }
    });
    this.renderEl.parentElement!.appendChild(ta);
    this.editEl = ta;
    ta.focus();
  }

  private setLatexEquivalent(newText: string): void {
    // code_block 内容即子文本节点——replaceWith 新 text
    const pos = this.getPos();
    if (pos === undefined) return;
    const { tr } = this.view.state;
    const $pos = this.view.state.doc.resolve(pos);
    const nodeStart = $pos.pos;
    tr.replaceWith(
      nodeStart + 1,
      nodeStart + 1 + this.node.content.size,
      newText ? this.view.state.schema.text(newText) : undefined as never,
    );
    this.view.dispatch(tr);
  }

  private commit(): void {
    if (!this.editing) return;
    const v = this.editEl?.value ?? '';
    this.setLatexEquivalent(v);
    this.exit();
  }

  private cancel(): void {
    if (!this.editing) return;
    this.exit();
  }

  private exit(): void {
    this.editEl?.remove();
    this.editEl = null;
    this.renderEl.style.display = '';
    this.editing = false;
    this.render();
  }

  update(node: ProseNode): boolean {
    if (node.type.name !== 'code_block') return false;
    const langChanged = String(node.attrs.language) !== String(this.node.attrs.language);
    if (langChanged && (this.lang() === 'mermaid' || node.attrs.language === 'mermaid')) {
      return false; // 语言进出 mermaid——重建 NodeView（切换默认面/渲染面）
    }
    const contentChanged = node.textContent !== this.node.textContent;
    this.node = node;
    if (this.lang() !== 'mermaid') return true; // 默认面——PM 自行同步 contentDOM
    if (contentChanged && !this.editing) this.render();
    return true;
  }

  ignoreMutation(): boolean {
    return !this.editing;
  }

  stopEvent(_e: Event): boolean {
    return this.editing;
  }

  destroy(): void {
    this.destroyed = true;
    this.cancel();
  }
}

/** nodeViews 装配项（auroraEditor 挂载用） */
export function mermaidNodeViews(): Record<string, (node: ProseNode, view: EditorView, getPos: () => number | undefined) => NodeView> {
  return {
    code_block: (node, view, getPos) => new MermaidNodeView(node, view, getPos),
  };
}
