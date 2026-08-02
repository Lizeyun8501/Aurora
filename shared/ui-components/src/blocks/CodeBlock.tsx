import type { ReactElement } from 'react';
import clsx from 'clsx';
import { asString, blockContent, type BlockComponentProps } from './content';

/** Renders a fenced code block with an optional language. */
export function CodeBlock({ block, className }: BlockComponentProps): ReactElement {
  const content = blockContent(block);
  const text = asString(content.text);
  const language = asString(content.language) || 'plaintext';
  return (
    <pre
      className={clsx('aurora-code-block', className)}
      data-block-id={block.id}
      data-language={language}
    >
      <code className={`language-${language}`}>{text}</code>
    </pre>
  );
}
