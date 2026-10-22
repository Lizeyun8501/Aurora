/**
 * DK-Zoom / R8：块级聚焦（zoom-in）——装饰插件。
 *
 * 语义：聚焦某个顶层块（paragraph/heading/code_block/list 等直接子块），
 * 其余顶层块 opacity 弱化，聚焦块 outline 高亮。文本照常可编辑
 * （这是「注意力聚焦」，不锁内容）。重复聚焦同一块或传 null 退出。
 *
 * 与 DK-2.7 highlightPlugin 同构：deco 全量重建（块级文档量级可接受），
 * 插件 state 只持顶层块 pos（deco 在 props.decorations 按当前 doc 现算——
 * 编辑中 doc 变化时 deco 自动跟随新 doc 重建，免映射）。
 * 外部 API：focusTopLevelBlock(view, blockPos | null)——blockPos 为顶层块
 * 在 doc 中的起始 pos（doc.forEach 的 offset）；getCurrentFocus 查询。
 */
import { Plugin, PluginKey, TextSelection } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';
import { Decoration, DecorationSet } from 'prosemirror-view';

export const focusBlockKey = new PluginKey<number | null>('focus-block');

/** 顶层块样式（幂等注入，对齐 highlight/math/mermaid 注入惯例） */
let styleInjected = false;
export function injectFocusBlockStyle(): void {
  if (styleInjected) return;
  styleInjected = true;
  const css = `
.aurora-zoom-dimmed { opacity: 0.14; transition: opacity 0.15s; }
.aurora-zoom-focus { outline: 1px solid rgba(88,166,255,0.45); border-radius: 6px; }
`;
  const el = document.createElement('style');
  el.textContent = css;
  document.head.appendChild(el);
}

export const focusBlockPlugin = new Plugin<number | null>({
  key: focusBlockKey,
  state: {
    init: (): number | null => null,
    apply: (tr, old): number | null => {
      const meta = tr.getMeta(focusBlockKey) as { pos: number | null } | undefined;
      return meta ? meta.pos : old;
    },
  },
  props: {
    decorations(state): DecorationSet {
      const pos = focusBlockKey.getState(state);
      if (pos == null) return DecorationSet.empty;
      return buildFocusDeco(state.doc, pos);
    },
  },
});

/** 构建聚焦 deco：聚焦块 outline，其余顶层块 dim */
function buildFocusDeco(
  doc: import('prosemirror-model').Node,
  blockPos: number,
): DecorationSet {
  const decos: Decoration[] = [];
  doc.forEach((node, offset) => {
    const end = offset + node.nodeSize;
    if (offset === blockPos) {
      decos.push(Decoration.node(offset, end, { class: 'aurora-zoom-focus' }));
    } else {
      decos.push(Decoration.node(offset, end, { class: 'aurora-zoom-dimmed' }));
    }
  });
  return DecorationSet.create(doc, decos);
}

/**
 * 聚焦/退出顶层块。blockPos=顶层块起始 pos；重复聚焦同一块=退出；
 * null=直接退出。聚焦时置选区到块首并滚动定位。
 */
export function focusTopLevelBlock(view: EditorView, blockPos: number | null): void {
  injectFocusBlockStyle();
  const prev = focusBlockKey.getState(view.state) ?? null;
  const next = blockPos != null && prev === blockPos ? null : blockPos;
  const tr = view.state.tr;
  tr.setMeta(focusBlockKey, { pos: next });
  if (next != null) {
    const $pos = view.state.doc.resolve(Math.min(next + 1, view.state.doc.content.size));
    tr.setSelection(TextSelection.near($pos));
    tr.scrollIntoView();
  }
  view.dispatch(tr);
}

/** 当前聚焦的顶层块 pos（未聚焦返回 null） */
export function getCurrentFocus(view: EditorView): number | null {
  return focusBlockKey.getState(view.state) ?? null;
}

/**
 * Mod-Shift-F：聚焦光标所在顶层块；已聚焦时=退出（keymap 装配用）。
 * 文档级选区（depth<1）视为退出指令。
 */
export function focusBlockShortcut(
  state: import('prosemirror-state').EditorState,
  dispatch: ((tr: import('prosemirror-state').Transaction) => void) | undefined,
  view?: EditorView,
): boolean {
  if (!view) return false;
  const { $from } = state.selection;
  if ($from.depth < 1) {
    if (focusBlockKey.getState(state) != null) {
      focusTopLevelBlock(view, null);
      return true;
    }
    return false;
  }
  focusTopLevelBlock(view, $from.before(1));
  void dispatch;
  return true;
}
