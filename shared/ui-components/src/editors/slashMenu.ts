/**
 * 斜杠命令菜单（E23 · 思源笔记 UX 之魂——自研实现,零 AGPL 代码）
 * 触发: 空行行首或空格后键入「/」→弹出块/插入全集菜单;后续字符过滤;Esc/删「/」关。
 * 实现: ProseMirror plugin decorations（PM 权威路径,与 React 渲染解耦）+
 * handleTextInput 触发态机;菜单点击/Enter 项=插入对应块/内容+删语法。
 */
import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';
import { insertBlock } from './insertCommands';

export const slashMenuKey = new PluginKey('auroraSlashMenu');

/** 菜单项（思源式全集——Aurora schema 适配） */
export interface SlashItem { id: string; label: string; hint: string; glyph: string; apply: (view: EditorView) => boolean }

export function slashItems(): SlashItem[] {
  return [
    { id: 'h1', label: '一级标题', hint: 'H1', glyph: 'H₁', apply: (v) => insertBlock(v, 'h1') },
    { id: 'h2', label: '二级标题', hint: 'H2', glyph: 'H₂', apply: (v) => insertBlock(v, 'h2') },
    { id: 'h3', label: '三级标题', hint: 'H3', glyph: 'H₃', apply: (v) => insertBlock(v, 'h3') },
    { id: 'bullet', label: '无序列表', hint: '• 列表', glyph: '•', apply: (v) => insertBlock(v, 'bullet') },
    { id: 'ordered', label: '有序列表', hint: '1. 列表', glyph: '1.', apply: (v) => insertBlock(v, 'ordered') },
    { id: 'task', label: '任务列表', hint: '☑ 待办', glyph: '☑', apply: (v) => insertBlock(v, 'task') },
    { id: 'quote', label: '引用块', hint: '❝ 引述', glyph: '❝', apply: (v) => insertBlock(v, 'quote') },
    { id: 'code', label: '代码块', hint: '</> 代码', glyph: '⌨', apply: (v) => insertBlock(v, 'code') },
    { id: 'table', label: '表格', hint: '▦ 表格', glyph: '▦', apply: (v) => insertBlock(v, 'table') },
    { id: 'divider', label: '分割线', hint: '— 分隔', glyph: '—', apply: (v) => insertBlock(v, 'divider') },
  ];
}

/** 触发判定: 行首或空格后的「/」——思源语义（段中无空格 / 不触发防 URL 路径误伤） */
function shouldTriggerSlash(textBefore: string, text: string): boolean {
  return text === '/' && (textBefore.length === 0 || /\s$/.test(textBefore));
}

export function slashMenuPlugin(onOpenChange: (open: boolean, query: string, pos: number) => void): Plugin {
  return new Plugin({
    key: slashMenuKey,
    props: {
      handleTextInput(view, from, _to, text) {
        if (view.composing) return false;
        const $from = view.state.doc.resolve(from);
        if ($from.parent.type.spec.code) return false;
        const before = $from.parent.textBetween(Math.max(0, $from.parentOffset - 200), $from.parentOffset, null, '\uFFFC');
        if (shouldTriggerSlash(before, text)) {
          onOpenChange(true, '', from + 1);
        }
        return false;
      },
      handleKeyDown(view, e) {
        const state = slashMenuKey.getState(view.state);
        if (!state || !state.open) return false;
        if (e.key === 'Escape') { onOpenChange(false, '', 0); return false; }
        return false;
      },
    },
    state: {
      init: () => ({ open: false }),
      apply(tr, prev) { const m = tr.getMeta(slashMenuKey); return { open: m?.open ?? prev.open }; },
    },
  });
}
