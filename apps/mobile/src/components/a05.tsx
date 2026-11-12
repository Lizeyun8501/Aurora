/**
 * a05 桥件 — B-02 切片三: A05BridgeEffect/breadcrumb/RichEditor 声明随 NoteEditor 迁出（纯搬移）。
 */
import React, { useEffect } from 'react';
import { platform } from '../adapters/androidPlatform';
import { setBlockRefResolver, BLOCK_REF_OPEN_EVENT } from '@aurora/ui-components'; // A-05: 块引用桥

export const RichEditor = React.lazy(() =>
  import('@aurora/ui-components').then((m) => ({ default: m.RichEditor })),
);
export const editorPlatform = platform as unknown as import('@aurora/ui-components').EditorPlatformBridge;

export const a05FetchBreadcrumb = async (noteId: string): Promise<{ id: string; title: string }[]> => {
  try {
    const b = (window as unknown as { AndroidBridge?: { breadcrumb?(id: string): string } }).AndroidBridge;
    if (!b?.breadcrumb) return [];
    return JSON.parse(b.breadcrumb(noteId)) as { id: string; title: string }[];
  } catch {
    return [];
  }
};

export function A05BridgeEffect(props: { noteId: string; onOpenNote: (noteId: string, title: string) => void }): null {
  const { onOpenNote } = props;
  useEffect(() => {
    setBlockRefResolver(async (nid, anchor) => {
      try {
        const b = (window as unknown as { AndroidBridge?: { resolveBlockRef?(id: string, a: string): string } }).AndroidBridge;
        if (!b?.resolveBlockRef) return { status: 'not_found' } as const;
        return JSON.parse(b.resolveBlockRef(nid, anchor ?? '')) as import('@aurora/ui-components').BlockRefData;
      } catch {
        return { status: 'not_found' } as const;
      }
    });
    const onOpen = (e: Event): void => {
      const d = (e as CustomEvent<{ noteId: string; anchor?: string | null }>).detail;
      if (d?.noteId) onOpenNote(d.noteId, d.noteId);
    };
    window.addEventListener(BLOCK_REF_OPEN_EVENT, onOpen);
    return () => window.removeEventListener(BLOCK_REF_OPEN_EVENT, onOpen);
  }, [onOpenNote]);
  return null;
}
