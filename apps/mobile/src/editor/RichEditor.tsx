// ProseMirror + Loro CRDT 富文本编辑器 — V19 §35（DEV-009）
//
// 初始化: platform.getNoteSnapshot(noteId) → LoroDoc.import → EditorView
// 保存:   debounce 1s → doc.export snapshot → platform.saveNoteSnapshot
//         （CRDT 合并语义，P2P 对端修改不丢失）
// 降级:   无 Android 桥（浏览器 mock）→ textarea 纯文本编辑
// 工具栏: V12 — 标题/加粗斜体下划线删除线/行内代码/列表/任务/引用/代码块/分割线

import { useEffect, useRef, useState, type CSSProperties } from 'react';
import type { EditorView } from 'prosemirror-view';
import type { EditorState } from 'prosemirror-state';
import { toggleMark, setBlockType, wrapIn } from 'prosemirror-commands';
import { wrapInList, liftListItem } from 'prosemirror-schema-list';

import { platform } from '../adapters/androidPlatform';
import {
  createAuroraEditor,
  loroDocFromBase64,
  bytesToBase64,
  undo,
  redo,
  AuroraEditorHandle,
} from './auroraEditor';
import { auroraSchema } from './schema';

export type EditorStatus = 'loading' | 'rich' | 'fallback';

interface RichEditorProps {
  noteId: string;
  /** 降级用纯文本初值（无桥时）。 */
  fallbackText: string;
  onDirty?: () => void;
  /** 保存完成回调（外层显示"已保存"）。 */
  onSaved?: () => void;
  /** 状态回调（外层显示"编辑器加载中/降级"）。 */
  onStatus?: (s: EditorStatus) => void;
}

// ---------------------------------------------------------------------------
// 降级模式 Markdown 工具条 — V23-I5 清单②（对齐富文本工具栏能力面）
// 受控 textarea: 全部走 setState, 光标经 rAF 恢复; 行前缀切换 + 选区包裹
// ---------------------------------------------------------------------------

/** 行前缀操作结果（text 新文本 / caret 新光标位）。 */
interface TextOp {
  text: string;
  caret: number;
}

/** 标题/列表/引用行前缀切换: 已有同级前缀 → 剥除回正文; 其他 # 级 → 替换。 */
function toggleLinePrefix(v: string, selStart: number, prefix: string | null): TextOp {
  const lineStart = v.lastIndexOf('\n', selStart - 1) + 1;
  const nlAt = v.indexOf('\n', selStart);
  const lineEnd = nlAt === -1 ? v.length : nlAt;
  const line = v.slice(lineStart, lineEnd);
  const bare = line.replace(/^#{1,6} |^[-*+] |^\d{1,9}\. |^> /, '');
  const newLine = prefix === null ? bare : prefix + bare;
  return {
    text: v.slice(0, lineStart) + newLine + v.slice(lineEnd),
    caret: lineStart + Math.max(newLine.length, 0),
  };
}

/** 选区包裹: 对称存在同标记 → 解开; 否则包裹（空选区填占位词）。 */
function wrapSelection(v: string, s: number, e: number, mark: string): TextOp {
  const before = v.slice(Math.max(0, s - mark.length), s);
  const after = v.slice(e, e + mark.length);
  if (before === mark && after === mark) {
    const inner = v.slice(s, e);
    return {
      text: v.slice(0, s - mark.length) + inner + v.slice(e + mark.length),
      caret: s - mark.length + inner.length,
    };
  }
  const inner = v.slice(s, e) || '文本';
  return {
    text: v.slice(0, s) + mark + inner + mark + v.slice(e),
    caret: s + mark.length + inner.length,
  };
}

// ---------------------------------------------------------------------------
// 工具栏命令（view 为空时按钮禁用）— PM 命令统一 (state, dispatch) 签名
// ---------------------------------------------------------------------------

type PMCommand = (
  state: EditorState,
  dispatch?: ((tr: import('prosemirror-state').Transaction) => void) | undefined,
  view?: EditorView,
) => boolean;

function runCmd(view: EditorView | null, cmd: PMCommand) {
  if (!view) return;
  const ok = cmd(view.state, view.dispatch);
  if (ok) view.focus();
}

function insertHorizontalRule(view: EditorView | null) {
  if (!view) return;
  const { state } = view;
  const tr = state.tr.replaceSelectionWith(auroraSchema.nodes.horizontal_rule.create());
  view.dispatch(tr);
  view.focus();
}
// 注: hr 按钮经 btn() 的 onClick 调 insertHorizontalRule(view)，与 runCmd 分离

/**
 * 列表切换 — 主流编辑器语义（非嵌套）:
 *  1. 光标已在目标类型列表内 → liftListItem 退出列表
 *  2. 光标在另一类型列表内（ul↔ol）→ setNodeMarkup 就地切换列表类型
 *  3. 非列表 → wrapInList 包裹
 * （prosemirror-schema-list 1.5 无 toggleList，自行组合）
 */
function makeToggleListCmd(
  listType: 'bullet_list' | 'ordered_list',
): PMCommand {
  const itemType = auroraSchema.nodes.list_item;
  const target = auroraSchema.nodes[listType];
  return (state, dispatch) => {
    const { $from } = state.selection;
    for (let d = $from.depth; d >= 0; d--) {
      const n = $from.node(d);
      if (n.type === target) {
        // 同类型 → 退出
        return liftListItem(itemType)(state, dispatch);
      }
      if (n.type.name === 'bullet_list' || n.type.name === 'ordered_list') {
        // 异类型 → 就地切换（bullet_list/ordered_list 的 content 均为 list_item+，兼容）
        if (dispatch) {
          dispatch(state.tr.setNodeMarkup($from.before(d), target).scrollIntoView());
        }
        return true;
      }
    }
    return wrapInList(target)(state, dispatch);
  };
}

/** 计算工具栏激活态（tick 变化触发重算）。 */
function useToolbarState(view: EditorView | null, _tick: number) {
  if (!view) {
    return { marks: new Set<string>(), blocks: new Set<string>(), canUndo: false, canRedo: false };
  }
  const { state } = view;
  const marks = new Set<string>();
  const selMarks = state.storedMarks ?? state.selection.$from.marks();
  for (const m of selMarks) marks.add(m.type.name);

  const blocks = new Set<string>();
  const { $from } = state.selection;
  for (let d = $from.depth; d >= 0; d--) {
    const n = $from.node(d);
    if (n.type.name === 'heading') blocks.add(`heading-${n.attrs.level}`);
    else blocks.add(n.type.name);
  }

  // PM 命令约定: dispatch 传 undefined 只检测可执行性
  const canUndo = (() => {
    try {
      return undo(state, undefined);
    } catch {
      return false;
    }
  })();
  const canRedo = (() => {
    try {
      return redo(state, undefined);
    } catch {
      return false;
    }
  })();

  return { marks, blocks, canUndo, canRedo };
}

// 工具栏按钮定义
type TBCmd = PMCommand;

const MARK_BTNS: Array<{ key: string; label: string; title: string; cmd: TBCmd }> = [
  { key: 'strong', label: 'B', title: '加粗', cmd: toggleMark(auroraSchema.marks.strong) },
  { key: 'em', label: 'I', title: '斜体', cmd: toggleMark(auroraSchema.marks.em) },
  { key: 'underline', label: 'U', title: '下划线', cmd: toggleMark(auroraSchema.marks.underline) },
  {
    key: 'strikethrough',
    label: 'S',
    title: '删除线',
    cmd: toggleMark(auroraSchema.marks.strikethrough),
  },
  { key: 'code', label: '</>', title: '行内代码', cmd: toggleMark(auroraSchema.marks.code) },
];

function blockCmd(type: string, attrs?: Record<string, unknown>): TBCmd {
  const nodeType = auroraSchema.nodes[type];
  return (state, dispatch) => {
    const { $from } = state.selection;
    // 再次点击同类型 → 回到正文段落
    const cur = $from.parent;
    if (cur.type.name === type && (!attrs || cur.attrs.level === attrs.level)) {
      return setBlockType(auroraSchema.nodes.paragraph)(state, dispatch);
    }
    return setBlockType(nodeType, attrs)(state, dispatch);
  };
}

const BLOCK_BTNS: Array<{ key: string; label: string; title: string; cmd: TBCmd }> = [
  { key: 'heading-1', label: 'H1', title: '标题 1', cmd: blockCmd('heading', { level: 1 }) },
  { key: 'heading-2', label: 'H2', title: '标题 2', cmd: blockCmd('heading', { level: 2 }) },
  { key: 'heading-3', label: 'H3', title: '标题 3', cmd: blockCmd('heading', { level: 3 }) },
  { key: 'code_block', label: '{ }', title: '代码块', cmd: blockCmd('code_block') },
  { key: 'task_block', label: '☑', title: '任务', cmd: blockCmd('task_block') },
];

const LIST_BTNS: Array<{ key: string; label: string; title: string; cmd: TBCmd }> = [
  {
    key: 'bullet_list',
    label: '•≡',
    title: '无序列表',
    cmd: makeToggleListCmd('bullet_list'),
  },
  {
    key: 'ordered_list',
    label: '1≡',
    title: '有序列表',
    cmd: makeToggleListCmd('ordered_list'),
  },
  {
    key: 'blockquote',
    label: '❝',
    title: '引用',
    cmd: wrapIn(auroraSchema.nodes.blockquote),
  },
];

// ---------------------------------------------------------------------------
// 工具栏组件
// ---------------------------------------------------------------------------

function EditorToolbar({ view, tick }: { view: EditorView | null; tick: number }) {
  const { marks, blocks, canUndo, canRedo } = useToolbarState(view, tick);

  const btn = (
    label: string,
    title: string,
    active: boolean,
    onClick: () => void,
    style?: CSSProperties,
  ) => (
    <button
      key={title}
      className={`tb-btn ${active ? 'active' : ''}`}
      title={title}
      onMouseDown={(e) => e.preventDefault()} // 保持编辑器焦点
      onClick={onClick}
      style={style}
    >
      {label}
    </button>
  );

  return (
    <div className="editor-toolbar" role="toolbar" aria-label="格式工具栏">
      {btn('↶', '撤销', canUndo, () => runCmd(view, undo))}
      {btn('↷', '重做', canRedo, () => runCmd(view, redo))}
      <span className="tb-sep" />
      {BLOCK_BTNS.map((b) => btn(b.label, b.title, blocks.has(b.key), () => runCmd(view, b.cmd)))}
      <span className="tb-sep" />
      {MARK_BTNS.map((b) =>
        btn(
          b.label,
          b.title,
          marks.has(b.key),
          () => runCmd(view, b.cmd),
          b.key === 'strong'
            ? { fontWeight: 800 }
            : b.key === 'em'
              ? { fontStyle: 'italic' }
              : b.key === 'underline'
                ? { textDecoration: 'underline' }
                : b.key === 'strikethrough'
                  ? { textDecoration: 'line-through' }
                  : undefined,
        ),
      )}
      <span className="tb-sep" />
      {LIST_BTNS.map((b) => btn(b.label, b.title, blocks.has(b.key), () => runCmd(view, b.cmd)))}
      <span className="tb-sep" />
      {btn('—', '分割线', false, () => insertHorizontalRule(view))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 主组件
// ---------------------------------------------------------------------------

export function RichEditor({ noteId, fallbackText, onDirty, onSaved, onStatus }: RichEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const handleRef = useRef<AuroraEditorHandle | null>(null);
  const dirtyRef = useRef(false);
  const [status, setStatus] = useState<EditorStatus>('loading');
  const [tick, setTick] = useState(0);
  const [fallbackContent, setFallbackContent] = useState(fallbackText);

  const changeStatus = (s: EditorStatus) => {
    setStatus(s);
    onStatus?.(s);
  };

  useEffect(() => {
    let cancelled = false;
    dirtyRef.current = false;
    changeStatus('loading');

    // 无桥 → 降级
    const snapB64 = platform.getNoteSnapshot(noteId);
    if (snapB64 === null) {
      changeStatus('fallback');
      return;
    }

    // 有桥：初始化 Loro + ProseMirror
    try {
      const doc = loroDocFromBase64(snapB64);
      if (!hostRef.current || cancelled) {
        doc.free?.();
        return;
      }

      const handle = createAuroraEditor(hostRef.current, {
        loroDoc: doc,
        onSave: (snapshotBytes) => {
          // Uint8Array → base64 → JNI（CRDT 合并到 Rust 侧 NoteDoc）
          const b64 = bytesToBase64(snapshotBytes);
          if (!platform.saveNoteSnapshot(noteId, b64)) {
            console.warn('saveNoteSnapshot failed for', noteId);
          } else {
            onSaved?.();
          }
          dirtyRef.current = false;
        },
        onUpdate: () => setTick((t) => t + 1),
      });
      if (cancelled) {
        handle.destroy();
        return;
      }
      handleRef.current = handle;
      changeStatus('rich');
    } catch (e) {
      console.error('editor init failed, falling back', e);
      if (!cancelled) changeStatus('fallback');
    }

    return () => {
      cancelled = true;
      const h = handleRef.current;
      if (h) {
        if (dirtyRef.current) h.flushSave();
        h.destroy();
        handleRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noteId]);

  // 降级 textarea
  if (status === 'fallback') {
    const ta = taRef.current;
    /** 统一文本操作入口: setState + 落盘 + rAF 恢复光标。 */
    const applyOp = (op: TextOp) => {
      setFallbackContent(op.text);
      onDirty?.();
      platform.saveNoteContent(noteId, op.text);
      requestAnimationFrame(() => {
        const el = taRef.current;
        if (el) {
          el.focus();
          el.setSelectionRange(op.caret, op.caret);
        }
      });
    };
    const lineOp = (prefix: string | null) => {
      if (!ta) return;
      applyOp(toggleLinePrefix(ta.value, ta.selectionStart, prefix));
    };
    const wrapOp = (mark: string) => {
      if (!ta) return;
      applyOp(wrapSelection(ta.value, ta.selectionStart, ta.selectionEnd, mark));
    };
    const fbBtn = (label: string, title: string, onClick: () => void) => (
      <button key={title} className="tb-btn" title={title} onMouseDown={(e) => e.preventDefault()} onClick={onClick}>
        {label}
      </button>
    );
    return (
      <div className="editor-fallback-wrap" style={{ flexDirection: 'column' }}>
        <textarea
          ref={taRef}
          className="editor-textarea"
          value={fallbackContent}
          onChange={(e) => {
            setFallbackContent(e.target.value);
            onDirty?.();
            platform.saveNoteContent(noteId, e.target.value);
          }}
          placeholder="开始写作…（降级模式：纯文本）"
        />
        <div className="editor-toolbar" role="toolbar" aria-label="Markdown 工具栏（降级模式）">
          {fbBtn('H1', '标题 1', () => lineOp('# '))}
          {fbBtn('H2', '标题 2', () => lineOp('## '))}
          {fbBtn('H3', '标题 3', () => lineOp('### '))}
          {fbBtn('正文', '回正文', () => lineOp(null))}
          <span className="tb-sep" />
          {fbBtn('B', '加粗', () => wrapOp('**'))}
          {fbBtn('I', '斜体', () => wrapOp('*'))}
          {fbBtn('</>', '行内代码', () => wrapOp('`'))}
          <span className="tb-sep" />
          {fbBtn('•≡', '无序列表', () => lineOp('- '))}
          {fbBtn('☑', '任务', () => lineOp('- [ ] '))}
          {fbBtn('❝', '引用', () => lineOp('> '))}
          <span className="tb-sep" />
          {fbBtn('—', '分割线', () => {
            if (!ta) return;
            const s = ta.selectionStart;
            applyOp({ text: ta.value.slice(0, s) + '\n---\n' + ta.value.slice(s), caret: s + 5 });
          })}
        </div>
      </div>
    );
  }

  return (
    <div className={`rich-editor-wrap ${status === 'loading' ? 'loading' : ''}`}>
      <div className="rich-editor-host" ref={hostRef} />
      {status === 'rich' && <EditorToolbar view={handleRef.current?.view ?? null} tick={tick} />}
      {status === 'loading' && (
        <div className="editor-loading-tip">
          <span className="spinner" />
          编辑器加载中…
        </div>
      )}
    </div>
  );
}
