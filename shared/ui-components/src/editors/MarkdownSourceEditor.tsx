/**
 * A-03 Markdown 源码模式（TA 2026-11-11 工单）——块视图 ⇄ 源码双向切换。
 *
 * 架构（shared 层组件+hook,双端宿主复用）:
 * - MarkdownSourceEditor: CodeMirror 6 本地打包（codemirror meta + lang-markdown,
 *   零 CDN/零外链——断网可用）;Mod-Enter 应用 / Esc 关闭由宿主接。
 * - useMarkdownSourceMode: 切换状态机——enter 快照 docToMd + 记录光标顶层块
 *   锚点;apply 走 mdToNodes（throw=非静默失败:E_SRC_PARSE 留源码态+放弃改动
 *   出口）;discard=放弃改动（PM doc 未动,直接回块视图）;回切后聚焦锚点块。
 * - 源码态禁止直接写库:仅 apply 成功后 dispatch 进 PM + 宿主 onCommit
 *   （flushSave→WritePath,ADR-003 禁旁路）——组件自身零保存通道。
 *
 * 停手边界: 不做源码态协同/Vim-Emacs 键位/语法扩展。
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactElement,
} from 'react';
import { EditorView as CMEditorView, basicSetup } from 'codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { EditorState as CMEditorState } from '@codemirror/state';
import { keymap as cmKeymap } from '@codemirror/view';
import type { EditorView as PMEditorView } from 'prosemirror-view';
import { TextSelection } from 'prosemirror-state';
import { docToMd, mdToNodes } from './auroraMarkdown';

/** 源码模式错误码（非静默回滚——用户可见语义）。 */
export const SOURCE_MODE_ERRORS = {
  /** mdToNodes 解析抛错（非零退出——留在源码态）。 */
  E_SRC_PARSE: 'E_SRC_PARSE',
  /** 编辑器未就绪（view 不可用——理论不可达,防御面）。 */
  E_SRC_NOVIEW: 'E_SRC_NOVIEW',
} as const;

export interface SourceModeError {
  code: string;
  message: string;
}

// ---------------------------------------------------------------------------
// 锚点工具：切换时记录光标顶层块序,回切后滚动+聚焦到对应块。
// 序（而非 pos）穿越往返——pos 在文档重建后语义失效,序在结构等价前提下稳定。
// ---------------------------------------------------------------------------

/** 光标所在顶层块序（depth<1 文档级选区返回 null——无锚点,回切不定位）。 */
export function topLevelIndexAtSelection(view: PMEditorView): number | null {
  const { $from } = view.state.selection;
  if ($from.depth < 1) return null;
  const pos = $from.before(1);
  let idx = -1;
  let found: number | null = null;
  view.state.doc.forEach((_n, offset) => {
    idx += 1;
    if (found === null && offset >= pos) found = idx;
  });
  return found;
}

/** 聚焦第 index 个顶层块（越界/文档变短——静默放弃定位,不报错不崩）。 */
export function focusTopLevelIndex(view: PMEditorView, index: number | null): void {
  if (index == null || index < 0) return;
  let i = -1;
  let target = -1;
  view.state.doc.forEach((_n, offset) => {
    i += 1;
    if (i === index) target = offset;
  });
  if (target < 0) return;
  const $pos = view.state.doc.resolve(Math.min(target + 1, view.state.doc.content.size));
  view.dispatch(view.state.tr.setSelection(TextSelection.near($pos)).scrollIntoView());
  view.focus();
}

// ---------------------------------------------------------------------------
// 切换状态机 hook
// ---------------------------------------------------------------------------

export interface UseMarkdownSourceModeResult {
  sourceMode: boolean;
  sourceMd: string;
  sourceError: SourceModeError | null;
  /** 进入源码模式（快照 md + 记锚点）。view 未就绪为 no-op。 */
  enter: () => void;
  /** 应用源码（mdToNodes→dispatch→onCommit→回块视图聚焦锚点）。
   * 解析失败: sourceError 置位,留在源码态,返回 false。 */
  apply: (md: string) => boolean;
  /** 放弃改动回块视图（PM doc 零触碰——无静默回滚:内容未变即无回滚）。 */
  discard: () => void;
}

export function useMarkdownSourceMode(
  view: PMEditorView | null,
  opts?: { onCommit?: () => void },
): UseMarkdownSourceModeResult {
  const [sourceMode, setSourceMode] = useState(false);
  const [sourceMd, setSourceMd] = useState('');
  const [sourceError, setSourceError] = useState<SourceModeError | null>(null);
  const anchorRef = useRef<number | null>(null);
  const commitRef = useRef(opts?.onCommit);
  commitRef.current = opts?.onCommit;

  const enter = useCallback((): void => {
    if (!view) return;
    anchorRef.current = topLevelIndexAtSelection(view);
    setSourceMd(docToMd(view.state.doc));
    setSourceError(null);
    setSourceMode(true);
  }, [view]);

  const backToBlocks = useCallback((): void => {
    setSourceMode(false);
    setSourceError(null);
    if (view) focusTopLevelIndex(view, anchorRef.current);
  }, [view]);

  const apply = useCallback(
    (md: string): boolean => {
      if (!view) {
        setSourceError({
          code: SOURCE_MODE_ERRORS.E_SRC_NOVIEW,
          message: '编辑器未就绪——无法应用',
        });
        return false;
      }
      try {
        const nodes = mdToNodes(view.state.schema, md);
        // 空 md（如全删）→ 单空段（doc 至少一块,零内容合法态非错误）。
        const filled = nodes.length
          ? nodes
          : [view.state.schema.nodes.paragraph.create()];
        view.dispatch(view.state.tr.replaceWith(0, view.state.doc.content.size, filled));
        commitRef.current?.(); // 宿主 flushSave——WritePath 唯一提交面
        setSourceMode(false);
        setSourceError(null);
        focusTopLevelIndex(view, anchorRef.current);
        return true;
      } catch (e: unknown) {
        // 非静默回滚:明确错误码+留源码态（内容不丢——放弃改动按钮退出）。
        setSourceError({
          code: SOURCE_MODE_ERRORS.E_SRC_PARSE,
          message: e instanceof Error ? e.message : String(e),
        });
        return false;
      }
    },
    [view],
  );

  const discard = useCallback((): void => {
    backToBlocks();
  }, [backToBlocks]);

  return { sourceMode, sourceMd, sourceError, enter, apply, discard };
}

// ---------------------------------------------------------------------------
// 展示组件:CM6 源码编辑器 + 应用/放弃 + 错误横幅
// ---------------------------------------------------------------------------

export interface MarkdownSourceEditorProps {
  /** 进入源码模式时的快照（docToMd 产物）。 */
  initialMd: string;
  /** 应用（宿主:mdToNodes+WritePath）。返回 false=留在源码态。 */
  onApply: (md: string) => boolean | void;
  /** 放弃改动（宿主:回块视图,PM doc 零触碰）。 */
  onDiscard: () => void;
  /** 宿主回传的解析错误（apply 失败产物——横幅展示,留在源码态）。 */
  error?: SourceModeError | null;
  /** A-10 补:源码态脏信号（docChanged 一次置真;apply/discard 由宿主清）——
   *  关窗确认消费（未提交源码态编辑禁静默丢弃——工单原文）。 */
  onDirty?: () => void;
  className?: string;
}

let sourceStyleInjected = false;
function injectSourceStyle(): void {
  if (sourceStyleInjected) return;
  sourceStyleInjected = true;
  const el = document.createElement('style');
  el.id = 'aurora-md-source-style';
  el.textContent = `
.aurora-md-source { display: flex; flex-direction: column; gap: 6px; }
.aurora-md-source .cm-editor { border: 1px solid rgba(88,166,255,0.35); border-radius: 8px; background: rgba(13,17,23,0.55); }
.aurora-md-source .cm-editor.cm-focused { outline: 2px solid rgba(88,166,255,0.55); }
.aurora-md-source .cm-scroller { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; line-height: 1.6; max-height: 60vh; }
.aurora-md-source-bar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.aurora-md-src-btn { font-size: 12px; padding: 4px 12px; border-radius: 999px; border: 1px solid rgba(88,166,255,0.4); background: rgba(88,166,255,0.12); cursor: pointer; }
.aurora-md-src-btn.danger { border-color: rgba(229,72,77,0.5); background: rgba(229,72,77,0.12); }
.aurora-md-src-err { border: 1px solid rgba(229,72,77,0.5); background: rgba(229,72,77,0.1); color: #ff8a8d; border-radius: 8px; padding: 8px 12px; font-size: 12px; }
`;
  document.head.appendChild(el);
}

export function MarkdownSourceEditor(props: MarkdownSourceEditorProps): ReactElement {
  const { initialMd, onApply, onDiscard, error, onDirty, className } = props;
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<CMEditorView | null>(null);
  const applyRef = useRef(onApply);
  applyRef.current = onApply;
  const onDirtyRef = useRef(onDirty);
  onDirtyRef.current = onDirty;
  const sourceDirtyRef = useRef(false);

  useEffect(() => {
    injectSourceStyle();
    const host = hostRef.current;
    if (!host) return;
    const state = CMEditorState.create({
      doc: initialMd,
      extensions: [
        basicSetup,
        markdown(),
        // A-10 补:源码态脏信号（首变更一次上报——关窗确认消费）
        CMEditorView.updateListener.of((vu) => {
          if (vu.docChanged && !sourceDirtyRef.current) {
            sourceDirtyRef.current = true;
            onDirtyRef.current?.();
          }
        }),
        cmKeymap.of([
          {
            key: 'Mod-Enter',
            preventDefault: true,
            run: (v: CMEditorView): boolean => {
              applyRef.current(v.state.doc.toString());
              return true;
            },
          },
        ]),
        CMEditorView.theme({ '&': { height: '100%' } }),
      ],
    });
    const view = new CMEditorView({ state, parent: host });
    viewRef.current = view;
    view.focus();
    return () => {
      viewRef.current = null;
      view.destroy();
    };
    // initialMd 快照语义:仅进入源码模式时建一次 CM 实例（重挂=重进）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleApply = useCallback((): void => {
    const md = viewRef.current ? viewRef.current.state.doc.toString() : initialMd;
    applyRef.current(md);
  }, [initialMd]);

  return (
    <div className={`aurora-md-source ${className ?? ''}`} role="region" aria-label="Markdown 源码编辑模式">
      <div className="aurora-md-source-bar">
        <button type="button" className="aurora-md-src-btn" onClick={handleApply} title="Mod-Enter">
          ✓ 应用并返回块视图
        </button>
        <button type="button" className="aurora-md-src-btn danger" onClick={onDiscard}>
          ✕ 放弃改动
        </button>
        <span style={{ fontSize: 11, opacity: 0.65 }}>
          源码态不自动保存——应用时经保存链路提交
        </span>
      </div>
      {error && (
        <div className="aurora-md-src-err" role="alert">
          <strong>应用失败（{error.code}）</strong>：{error.message}
          ——源码已保留,可修正后重试或放弃改动。
        </div>
      )}
      <div ref={hostRef} />
    </div>
  );
}
