import type { ReactElement } from 'react';
import clsx from 'clsx';
import type { BlockComponentProps } from './content';

/** Renders a horizontal-rule divider block. */
export function DividerBlock({ block, className }: BlockComponentProps): ReactElement {
  return (
    <hr className={clsx('aurora-divider-block', className)} data-block-id={block.id} />
  );
}
