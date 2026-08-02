import type { ReactElement } from 'react';
import clsx from 'clsx';
import { asBoolean, asString, blockContent, type BlockComponentProps } from './content';

/** Renders a to-do checkbox block reflecting `content.checked`. */
export function TodoItemBlock({ block, className }: BlockComponentProps): ReactElement {
  const content = blockContent(block);
  const text = asString(content.text);
  const checked = asBoolean(content.checked);
  return (
    <label
      className={clsx('aurora-todo-item-block', checked && 'checked', className)}
      data-block-id={block.id}
    >
      <input type="checkbox" checked={checked} readOnly disabled />
      <span className="aurora-todo-text">{text}</span>
    </label>
  );
}
