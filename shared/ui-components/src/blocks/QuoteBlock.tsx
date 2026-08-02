import type { ReactElement } from 'react';
import clsx from 'clsx';
import {
  asNullableString,
  asString,
  blockContent,
  type BlockComponentProps,
} from './content';

/** Renders a blockquote with optional `cite` attribution. */
export function QuoteBlock({ block, className }: BlockComponentProps): ReactElement {
  const content = blockContent(block);
  const text = asString(content.text);
  const cite = asNullableString(content.cite);
  return (
    <blockquote
      className={clsx('aurora-quote-block', className)}
      data-block-id={block.id}
      cite={cite ?? undefined}
    >
      {text}
    </blockquote>
  );
}
