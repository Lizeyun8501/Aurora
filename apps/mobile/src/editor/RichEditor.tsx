// ProseMirror + Loro CRDT 富文本编辑器 — V19 §35（DEV-009）
//
// 初始化: platform.getNoteSnapshot(noteId) → LoroDoc.import → EditorView
// 保存:   debounce 1s → doc.export snapshot → platform.saveNoteSnapshot
//         （CRDT 合并语义，P2P 对端修改不丢失）
// 降级:   无 Android 桥（浏览器 mock）→ textarea 纯文本编辑

import { useEffect, useRef, useState } from 'react';

import { platform } from '../adapters/androidPlatform';
import {
  createAuroraEditor,
  loroDocFromBase64,
  bytesToBase64,
  extractPlainText,
  AuroraEditorHandle,
} from './auroraEditor';

interface RichEditorProps {
  noteId: string;
  /** 降级用纯文本初值（无桥时）。 */
  fallbackText: string;
  onDirty?: () => void;
}

export function RichEditor({ noteId, fallbackText, onDirty }: RichEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const handleRef = useRef<AuroraEditorHandle | null>(null);
  const dirtyRef = useRef(false);
  const [status, setStatus] = useState<'loading' | 'rich' | 'fallback'>('loading');
  const [fallbackContent, setFallbackContent] = useState(fallbackText);

  useEffect(() => {
    let cancelled = false;
    dirtyRef.current = false;

    // 无桥 → 降级
    const snapB64 = platform.getNoteSnapshot(noteId);
    if (snapB64 === null) {
      setStatus('fallback');
      return;
    }

    // 有桥：初始化 Loro + ProseMirror
    try {
      const doc = loroDocFromBase64(snapB64);
      if (!hostRef.current || cancelled) {
        doc.free?.();
        return;
      }

      const handle = createAuroraEditor({
        doc,
        host: hostRef.current,
        onSave: (b64) => {
          if (!platform.saveNoteSnapshot(noteId, b64)) {
            console.warn('saveNoteSnapshot failed for', noteId);
          }
          dirtyRef.current = false;
        },
        onDirty: () => {
          dirtyRef.current = true;
          onDirty?.();
        },
      });
      if (cancelled) {
        handle.destroy();
        return;
      }
      handleRef.current = handle;
      setStatus('rich');
    } catch (e) {
      console.error('editor init failed, falling back', e);
      if (!cancelled) setStatus('fallback');
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
    return (
      <textarea
        className="editor-textarea"
        value={fallbackContent}
        onChange={(e) => {
          setFallbackContent(e.target.value);
          onDirty?.();
          platform.saveNoteContent(noteId, e.target.value);
        }}
        placeholder="开始写作…（降级模式：纯文本）"
      />
    );
  }

  return (
    <div
      className={`rich-editor-host ${status === 'loading' ? 'loading' : ''}`}
      ref={hostRef}
    />
  );
}
