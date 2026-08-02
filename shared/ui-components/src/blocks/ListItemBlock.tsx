import type { ReactElement } from 'react';
import clsx from 'clsx';
import { asBoolean, asString, blockContent, type BlockComponentProps } from './content';

/**
 * Renders a single list-item block. Uses a `role="listitem"` container (rather
 * than a bare `<li>`) so it remains valid DOM when rendered standalone or
 * nested inside the block tree. `content.ordered` toggles the marker style.
 */
export function ListItemBlock({ block, className }: BlockComponentProps): ReactElement {
  const content = blockContent(block);
  const text = asString(content.text);
  const ordered = asBoolean(content.ordered);
  return (
    <div
      role="listitem"
      className={clsx('aurora-list-item-block', ordered && 'ordered', className)}
      data-block-id={block.id}
    >
      <span className="aurora-list-marker" aria-hidden="true">
        {ordered ? '#' : '•'}
      </span>
      <span className="aurora-list-text">{text}</span>
    </div>
  );
}
