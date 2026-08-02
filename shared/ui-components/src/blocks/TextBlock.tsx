import type { ReactElement } from 'react';
import clsx from 'clsx';
import { blockText, type BlockComponentProps } from './content';

/** Renders a plain paragraph text block. */
export function TextBlock({ block, className }: BlockComponentProps): ReactElement {
  return (
    <p className={clsx('aurora-text-block', className)} data-block-id={block.id}>
      {blockText(block)}
    </p>
  );
}
