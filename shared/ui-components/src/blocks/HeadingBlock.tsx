import type { ReactElement } from 'react';
import clsx from 'clsx';
import { asNumber, blockContent, blockText, type BlockComponentProps } from './content';

type HeadingLevel = 1 | 2 | 3 | 4 | 5 | 6;
const VALID_LEVELS: ReadonlySet<number> = new Set([1, 2, 3, 4, 5, 6]);

function resolveLevel(block: BlockComponentProps['block']): HeadingLevel {
  const level = asNumber(blockContent(block).level);
  return VALID_LEVELS.has(level) ? (level as HeadingLevel) : 1;
}

/** Renders a heading block (h1–h6) based on `content.level`. */
export function HeadingBlock({ block, className }: BlockComponentProps): ReactElement {
  const level = resolveLevel(block);
  const text = blockText(block);
  const commonProps = {
    className: clsx('aurora-heading-block', `aurora-heading-${level}`, className),
    'data-block-id': block.id,
  };
  switch (level) {
    case 1:
      return <h1 {...commonProps}>{text}</h1>;
    case 2:
      return <h2 {...commonProps}>{text}</h2>;
    case 3:
      return <h3 {...commonProps}>{text}</h3>;
    case 4:
      return <h4 {...commonProps}>{text}</h4>;
    case 5:
      return <h5 {...commonProps}>{text}</h5>;
    default:
      return <h6 {...commonProps}>{text}</h6>;
  }
}
