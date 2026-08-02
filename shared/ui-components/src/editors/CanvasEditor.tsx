import { useState, type ReactElement } from 'react';
import { motion } from 'framer-motion';
import type { Block } from '@aurora/shared-types';
import clsx from 'clsx';
import { BlockRenderer } from '../blocks/BlockRenderer';

/** A block placed on the canvas with absolute coordinates. */
export interface CanvasBlock {
  block: Block;
  x: number;
  y: number;
  width?: number;
}

export interface CanvasEditorProps {
  blocks?: CanvasBlock[];
  onChange?: (blocks: CanvasBlock[]) => void;
  className?: string;
}

/**
 * Free-form canvas editor (SIMPLIFIED).
 *
 * Blocks are positioned absolutely inside a scrollable container and made
 * draggable via framer-motion's `drag`. On drag end the block's x/y are
 * updated and `onChange` is emitted. This intentionally omits connectors,
 * multi-select, resize handles, zoom/pan, and virtualization that a
 * production canvas would require.
 */
export function CanvasEditor({
  blocks: initial = [],
  onChange,
  className,
}: CanvasEditorProps): ReactElement {
  const [blocks, setBlocks] = useState<CanvasBlock[]>(initial);

  const handleDragEnd = (index: number, offset: { x: number; y: number }): void => {
    setBlocks((prev) => {
      const next = prev.map((b, i) =>
        i === index ? { ...b, x: b.x + offset.x, y: b.y + offset.y } : b,
      );
      onChange?.(next);
      return next;
    });
  };

  return (
    <div
      className={clsx('aurora-canvas-editor', className)}
      role="region"
      aria-label="Canvas editor"
    >
      {blocks.map((item, index) => (
        <motion.div
          key={item.block.id}
          drag
          dragMomentum={false}
          className="aurora-canvas-block"
          style={{
            position: 'absolute',
            left: item.x,
            top: item.y,
            width: item.width,
          }}
          onDragEnd={(_, info) =>
            handleDragEnd(index, { x: info.offset.x, y: info.offset.y })
          }
        >
          <BlockRenderer block={item.block} />
        </motion.div>
      ))}
    </div>
  );
}
