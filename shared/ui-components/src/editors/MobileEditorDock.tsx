/**
 * R19 移动端编辑器 Dock + 块插入面板 — MobileEditorDock / BlockInsertPanel
 *
 * 任务书：思源移动端对标（交互语义对标，AGPL 红线——零思源源码/组件结构参考，
 * 全部走 Aurora 共享层自有体系）。
 *
 * - MobileEditorDock：底部常驻工具栏（+/撤销/重做/块上移/块下移）
 *   · 撤销/重做状态感知复用 useToolbarState（canUndo/canRedo）
 *   · fixed 贴底：bottom = max(--kbd-inset, safe-area)（键盘态视口变量——
 *     共享层只读 CSS 变量，勿 import mobile 模块——方向纪律）
 * - BlockInsertPanel：+ 弹两列网格块菜单（H1-H6/列表/任务/引述/代码/表格/分割线）
 *   · 底部 sheet + 遮罩（transition 180ms ≤ 200ms）
 *   · 图标：字符+文字标签（EditorToolbar 惯例——零图标库依赖，包体红线友好）
 *
 * React #310 纪律（v24.1 真机教训）：本文件所有组件 hooks 声明无条件
 * early return 之前——MobileEditorDock 的 useState/useToolbarState 全部
 * 顶部声明，无条件分支 return。
 */
import React, { useEffect, useState } from 'react';
import type { EditorView } from 'prosemirror-view';
import { undo, redo } from './auroraEditor';
import { insertBlock, moveBlock, type BlockKind } from './insertCommands';
import { useToolbarState } from './RichEditor';

/** Dock/Sheet 样式（幂等注入——对齐 focusBlock/highlight 惯例）。 */
let styleInjected = false;
export function injectMobileDockStyle(): void {
  if (styleInjected) return;
  styleInjected = true;
  const el = document.createElement('style');
  el.textContent = `
.m-dock {
  position: fixed; left: 0; right: 0; z-index: 60;
  bottom: max(var(--kbd-inset, 0px), env(safe-area-inset-bottom));
  display: flex; justify-content: space-around; align-items: center;
  height: 52px; background: var(--m-dock-bg, #fff);
  border-top: 1px solid rgba(0,0,0,0.08);
  box-shadow: 0 -2px 12px rgba(0,0,0,0.06);
}
.m-dock button {
  min-width: 48px; min-height: 44px; /* 触屏热区 ≥44px */
  border: none; background: none; font-size: 20px; line-height: 1;
  color: var(--m-dock-fg, #333); border-radius: 8px; cursor: pointer;
}
.m-dock button:disabled { opacity: 0.35; }
.m-dock button.m-dock-plus { font-size: 24px; font-weight: 600; }
.m-sheet-mask {
  position: fixed; inset: 0; z-index: 61; background: rgba(0,0,0,0.28);
  opacity: 0; transition: opacity 180ms ease; pointer-events: none;
}
.m-sheet-mask.open { opacity: 1; pointer-events: auto; }
.m-sheet {
  position: fixed; left: 0; right: 0; bottom: 0; z-index: 62;
  transform: translateY(100%); transition: transform 180ms ease;
  background: var(--m-sheet-bg, #fff); border-radius: 16px 16px 0 0;
  padding: 10px 14px calc(14px + env(safe-area-inset-bottom));
  max-height: 60vh; overflow-y: auto;
  pointer-events: none; /* closed 姿态禁 hit-test（保险） */
}
.m-sheet.open { transform: translateY(0); pointer-events: auto; }
.m-sheet-title { font-size: 12px; color: #888; padding: 4px 2px 8px; }
.m-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.m-grid button {
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  min-height: 64px; border: 1px solid rgba(0,0,0,0.08); border-radius: 10px;
  background: rgba(0,0,0,0.02); cursor: pointer; padding: 8px 4px;
}
.m-grid button:active { background: rgba(0,0,0,0.08); }
.m-grid .m-icon { font-size: 20px; line-height: 1.2; }
.m-grid .m-label { font-size: 12px; color: #555; margin-top: 4px; }
`;
  document.head.appendChild(el);
}

interface DockProps {
  view: EditorView | null;
  /** useToolbarState 消费的 tick（RichEditor onUpdate 驱动重渲染） */
  tick: number;
}

/**
 * 底部常驻工具栏。挂载方：RichEditor（platform=mobile 态）。
 * hooks 声明全前置（React #310 纪律——无 early return）。
 */
export function MobileEditorDock({ view, tick }: DockProps): React.ReactElement | null {
  // React #310：全部 hooks 无条件声明（含 view=null 早期态）
  const [panelOpen, setPanelOpen] = useState(false);
  // 撤销/重做状态感知：复用 RichEditor useToolbarState
  // （PM 命令约定 dispatch=undefined 只探测可执行性）
  const { canUndo, canRedo } = useToolbarState(view, tick);
  const [atFirst, setAtFirst] = useState(false);
  const [atLast, setAtLast] = useState(false);

  useEffect(() => {
    injectMobileDockStyle();
  }, []);

  // 光标顶层块位置（上移/下移禁用态）
  useEffect(() => {
    if (!view) return;
    const { $from } = view.state.selection;
    if ($from.depth < 1) return;
    const selfPos = $from.before(1);
    let first = -1;
    let last = -1;
    view.state.doc.forEach((_node, offset) => {
      if (first < 0) first = offset;
      last = offset;
    });
    setAtFirst(selfPos === first);
    setAtLast(selfPos === last);
  }, [view, tick]);

  if (!view) return null;

  const btn = (
    label: string,
    title: string,
    onClick: () => void,
    disabled = false,
    extraClass = '',
  ) => (
    <button type="button" title={title} aria-label={title} disabled={disabled}
      className={extraClass}
      onMouseDown={(e) => e.preventDefault()} /* 保持编辑器焦点 */
      onClick={onClick}>
      {label}
    </button>
  );

  return (
    <>
      <BlockInsertPanel view={view} open={panelOpen} onClose={() => setPanelOpen(false)} />
      <div className="m-dock" role="toolbar" aria-label="移动端编辑工具栏">
        {btn('+', '插入块', () => setPanelOpen((v) => !v), false, 'm-dock-plus')}
        {btn('↶', '撤销', () => { undo(view.state, view.dispatch); view.focus(); }, !canUndo)}
        {btn('↷', '重做', () => { redo(view.state, view.dispatch); view.focus(); }, !canRedo)}
        {btn('↑', '块上移', () => moveBlock(view, -1), atFirst)}
        {btn('↓', '块下移', () => moveBlock(view, 1), atLast)}
      </div>
    </>
  );
}

/** 菜单项（字符图标 + 文字标签——零图标库）。 */
const MENU_ITEMS: Array<{ kind: BlockKind; icon: string; label: string }> = [
  { kind: 'h1', icon: 'H1', label: '标题一' },
  { kind: 'h2', icon: 'H2', label: '标题二' },
  { kind: 'h3', icon: 'H3', label: '标题三' },
  { kind: 'h4', icon: 'H4', label: '标题四' },
  { kind: 'h5', icon: 'H5', label: '标题五' },
  { kind: 'h6', icon: 'H6', label: '标题六' },
  { kind: 'bullet', icon: '•', label: '无序列表' },
  { kind: 'ordered', icon: '1.', label: '有序列表' },
  { kind: 'task', icon: '☑', label: '任务列表' },
  { kind: 'quote', icon: '❝', label: '引述块' },
  { kind: 'code', icon: '</>', label: '代码块' },
  { kind: 'table', icon: '▦', label: '表格' },
  { kind: 'divider', icon: '—', label: '分割线' },
];

interface PanelProps {
  view: EditorView | null;
  open: boolean;
  onClose: () => void;
}

/**
 * 两列网格块菜单（底部 sheet）。hooks 前置（React #310）。
 * open=false 仍渲染（closed 姿态 translateY(100%) 藏底）——保证开/关
 * transition 播放（mount 即 open 无状态变化则动画不触发）。
 * 插入成功即关面板回焦编辑器。
 */
export function BlockInsertPanel({ view, open, onClose }: PanelProps): React.ReactElement | null {
  useEffect(() => {
    injectMobileDockStyle();
  }, []);
  if (!view) return null;

  const insert = (kind: BlockKind) => {
    insertBlock(view, kind);
    onClose();
  };

  return (
    <>
      <div className={`m-sheet-mask${open ? ' open' : ''}`} role="presentation"
        onClick={onClose} aria-hidden="true" />
      <div className={`m-sheet${open ? ' open' : ''}`} role="dialog" aria-label="插入块面板">
        <div className="m-sheet-title">插入块</div>
        <div className="m-grid">
          {MENU_ITEMS.map((m) => (
            <button type="button" key={m.kind} onClick={() => insert(m.kind)}
              aria-label={m.label}>
              <span className="m-icon">{m.icon}</span>
              <span className="m-label">{m.label}</span>
            </button>
          ))}
        </div>
      </div>
    </>
  );
}
