import type { ReactElement } from 'react';
import type { Block, BlockType } from '@aurora/shared-types';
import clsx from 'clsx';
import type { BlockComponentProps } from './content';
import { TextBlock } from './TextBlock';
import { HeadingBlock } from './HeadingBlock';
import { CodeBlock } from './CodeBlock';
import { ImageBlock } from './ImageBlock';
import { TableBlock } from './TableBlock';
import { DividerBlock } from './DividerBlock';
import { QuoteBlock } from './QuoteBlock';
import { ListItemBlock } from './ListItemBlock';
import { TodoItemBlock } from './TodoItemBlock';

/** A block renderer component: takes `{ block }` and returns ReactElement. */
export type BlockComponent = (props: BlockComponentProps) => ReactElement;

/** Registry of custom renderers keyed by `BlockType` (for plugin-registered types). */
export type BlockRendererMap = Partial<Record<BlockType, BlockComponent>>;

export interface BlockRendererProps {
  block: Block;
  /** Optional custom renderers; take precedence over the built-in renderers. */
  blockRenderers?: BlockRendererMap;
}

/** Built-in renderers for the 9 core block types. */
const BUILTIN_RENDERERS: Record<string, BlockComponent> = {
  text: TextBlock,
  heading: HeadingBlock,
  code: CodeBlock,
  image: ImageBlock,
  table: TableBlock,
  divider: DividerBlock,
  quote: QuoteBlock,
  list_item: ListItemBlock,
  todo_item: TodoItemBlock,
};

function UnknownBlock({ block, className }: BlockComponentProps): ReactElement {
  return (
    <div
      className={clsx('aurora-block', 'aurora-block-unknown', className)}
      data-block-id={block.id}
      data-block-type={block.block_type}
    >
      Unsupported block type: {block.block_type}
    </div>
  );
}

/**
 * Dispatcher that renders the appropriate component for a `Block` based on its
 * `block_type`. Custom renderers supplied via `blockRenderers` override the
 * built-ins (useful for plugin-registered block types). Child blocks are
 * rendered recursively beneath the parent.
 */
export function BlockRenderer({ block, blockRenderers }: BlockRendererProps): ReactElement {
  const custom = blockRenderers?.[block.block_type];
  const Renderer = custom ?? BUILTIN_RENDERERS[block.block_type] ?? UnknownBlock;
  return (
    <>
      <Renderer block={block} />
      {block.children.length > 0 && (
        <div className="aurora-block-children" data-block-id-children={block.id}>
          {block.children.map((child) => (
            <BlockRenderer key={child.id} block={child} blockRenderers={blockRenderers} />
          ))}
        </div>
      )}
    </>
  );
}
