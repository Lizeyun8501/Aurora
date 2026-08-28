// V19 §35.2 Loro-ProseMirror 集成 — createAuroraEditor（DEV-009）
//
// 数据链路:
//   ProseMirror 事务 → LoroSyncPlugin → LoroDoc 容器（JS/WASM）
//     → debounce 1s → export snapshot → AndroidBridge.saveNoteSnapshot
//       → JNI → Rust NoteDoc 合并导入 → KVStore notesnap:{id}
//
// 与 Rust 侧 NoteDoc 的关系: 同一 LoroDoc 的两个副本（JS WASM + Rust native），
// 通过快照单向（JS→Rust）合并；P2P 同步在 Rust 侧进行，下次打开笔记时
// 经 getNoteSnapshot 快照恢复（拉取 Rust 侧合并结果）。

import { EditorState } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { keymap } from 'prosemirror-keymap';
import { baseKeymap } from 'prosemirror-commands';
import { splitListItem } from 'prosemirror-schema-list';
import { LoroDoc } from 'loro-crdt';
import {
  LoroSyncPlugin,
  LoroUndoPlugin,
  undo,
  redo,
  canUndo,
  canRedo,
} from 'loro-prosemirror';

import { auroraSchema } from './schema';

export interface AuroraEditorHandle {
  view: EditorView;
  doc: LoroDoc;
  /** 立即保存（丢弃 debounce 计时）。 */
  flushSave: () => void;
  /** 销毁视图与订阅。 */
  destroy: () => void;
}

export interface CreateAuroraEditorOptions {
  /** 初始 LoroDoc（已导入快照）。 */
  loroDoc: LoroDoc;
  /** 保存回调（debounce 1s，V19 §35.2 scheduleSave）。 */
  onSave: (snapshot: Uint8Array) => void;
  /** 快照导出工厂（默认 doc.export({ mode: 'snapshot' })）。 */
  exportSnapshot?: (doc: LoroDoc) => Uint8Array;
  /** 每次事务后回调（工具栏刷新激活态用）。 */
  onUpdate?: () => void;
}

/**
 * 创建 Aurora 编辑器视图 — V19 §35.2。
 *
 * 插件栈:
 * - LoroSyncPlugin: ProseMirror Doc ↔ LoroDoc 双向同步（编辑操作自动合并）
 * - LoroUndoPlugin: 基于 Loro UndoManager 的协同安全撤销
 * - keymap: Mod-z / Mod-y / Mod-Shift-z
 */
export function createAuroraEditor(
  dom: HTMLElement,
  options: CreateAuroraEditorOptions,
): AuroraEditorHandle {
  const { loroDoc, onSave, exportSnapshot, onUpdate } = options;

  // 初始内容: loro-prosemirror 的 view 钩子（setTimeout 0）会做初始同步 —
  // map.size>0 时自动 createNodeFromLoroObj 转换；空文档走 delete 分支。
  // 此处按官方示例用 PM 默认空文档（避免提前触碰 "doc" 容器产生副作用）。
  const state = EditorState.create({
    schema: auroraSchema,
    plugins: [
      // loro-prosemirror 的类型签名固定了容器形状，与通用 LoroDoc 泛型不兼容（运行时无差异）
      LoroSyncPlugin({ doc: loroDoc as unknown as Parameters<typeof LoroSyncPlugin>[0]['doc'] }),
      LoroUndoPlugin({ doc: loroDoc }),
      keymap({
        'Mod-z': undo,
        'Mod-y': redo,
        'Mod-Shift-z': redo,
        // 列表内 Enter = 拆分列表项（空项 Enter = 提升退出）— 标准列表行为，
        // 必须置于 baseKeymap 之前（keymap 先绑定优先级高），
        // 否则 Enter 拆的是 li 内段落，产生 li>p+p 的非预期结构
        Enter: splitListItem(auroraSchema.nodes.list_item),
      }),
      keymap(baseKeymap), // Enter/Backspace/方向键等基础编辑行为
    ],
  });

  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  const doExport = () =>
    (exportSnapshot ?? ((d: LoroDoc) => d.export({ mode: 'snapshot' } as never) as Uint8Array))(
      loroDoc,
    );

  const scheduleSave = () => {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      try {
        onSave(doExport());
      } catch (e) {
        console.error('loro snapshot export failed', e);
      }
    }, 1000); // 1秒 debounce（V19 §35.2）
  };

  const view = new EditorView(dom, {
    state,
    dispatchTransaction(tx) {
      view.updateState(view.state.apply(tx));
      // debounce 保存
      scheduleSave();
      onUpdate?.();
    },
  });

  // 订阅本地更新 → 触发 debounce 保存（含 import 引起的变化时由 flush 语义兜底）
  const unsubscribe = loroDoc.subscribeLocalUpdates(() => {
    scheduleSave();
  });

  return {
    view,
    doc: loroDoc,
    flushSave() {
      if (saveTimer) {
        clearTimeout(saveTimer);
        saveTimer = null;
      }
      onSave(doExport());
    },
    destroy() {
      if (saveTimer) clearTimeout(saveTimer);
      try {
        unsubscribe();
      } catch {
        /* noop */
      }
      view.destroy();
      // 释放 WASM 侧 LoroDoc — 每次打开笔记 new LoroDoc，不 free 会累积泄漏
      try {
        loroDoc.free?.();
      } catch {
        /* 已释放（StrictMode 双执行） */
      }
    },
  };
}

/** 从 base64 快照恢复 LoroDoc（空/损坏时新建空文档）。 */
export function loroDocFromBase64(b64: string | null | undefined): LoroDoc {
  const doc = new LoroDoc();
  if (b64) {
    try {
      const bytes = base64ToBytes(b64);
      if (bytes.length > 0) {
        doc.import(bytes);
      }
    } catch (e) {
      console.warn('loro snapshot import failed, starting fresh', e);
    }
  }
  return doc;
}

/** Uint8Array → base64（分块避免栈溢出）。 */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** base64 → Uint8Array。 */
export function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** 提取文档纯文本（保存到 NoteDoc body 容器，供列表预览/搜索）。 */
export function extractPlainText(doc: LoroDoc): string {
  try {
    const text = doc.getText('body');
    return text.toString();
  } catch {
    return '';
  }
}

export { undo, redo, canUndo, canRedo };
