import { useCallback, useRef, useState } from 'react';
import type { Block } from '@aurora/shared-types';

export interface UseBlockOptions {
  /** Initial block value (e.g. fetched from a store). */
  initialBlock?: Block | null;
  /** Called with the patch whenever `update` is invoked. */
  onUpdate?: (blockId: string, updates: Partial<Block>) => void;
  /** Called when `remove` is invoked. */
  onRemove?: (blockId: string) => void;
  /** Called when a child is added via `addChild`. */
  onAddChild?: (parentId: string, child: Block) => void;
}

export interface UseBlockResult {
  block: Block | null;
  update: (updates: Partial<Block>) => void;
  remove: () => void;
  addChild: (child: Block) => void;
}

/**
 * Manages a single block via a callback API. Local state mirrors the block;
 * `onUpdate` / `onRemove` / `onAddChild` callbacks let the caller persist
 * changes to a store or backend.
 */
export function useBlock(blockId: string, options?: UseBlockOptions): UseBlockResult {
  const [block, setBlock] = useState<Block | null>(options?.initialBlock ?? null);
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const update = useCallback(
    (updates: Partial<Block>) => {
      setBlock((prev) => {
        if (!prev) return prev;
        const next: Block = {
          ...prev,
          ...updates,
          updated_at: new Date().toISOString(),
        };
        optionsRef.current?.onUpdate?.(blockId, updates);
        return next;
      });
    },
    [blockId],
  );

  const remove = useCallback(() => {
    optionsRef.current?.onRemove?.(blockId);
    setBlock(null);
  }, [blockId]);

  const addChild = useCallback(
    (child: Block) => {
      setBlock((prev) => {
        if (!prev) return prev;
        const next: Block = { ...prev, children: [...prev.children, child] };
        optionsRef.current?.onAddChild?.(blockId, child);
        return next;
      });
    },
    [blockId],
  );

  return { block, update, remove, addChild };
}
