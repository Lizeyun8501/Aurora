import type { ReactElement } from 'react';
import clsx from 'clsx';
import {
  asNullableString,
  asString,
  blockContent,
  type BlockComponentProps,
} from './content';

/** Renders an image block with an optional caption. */
export function ImageBlock({ block, className }: BlockComponentProps): ReactElement {
  const content = blockContent(block);
  const url = asString(content.url);
  const alt = asString(content.alt);
  const caption = asNullableString(content.caption);
  return (
    <figure className={clsx('aurora-image-block', className)} data-block-id={block.id}>
      {url ? (
        <img src={url} alt={alt} className="aurora-image" />
      ) : (
        <div className="aurora-image-placeholder">No image source</div>
      )}
      {caption && <figcaption>{caption}</figcaption>}
    </figure>
  );
}
