import type { ReactElement } from 'react';
import clsx from 'clsx';
import {
  asStringArray,
  asStringMatrix,
  blockContent,
  type BlockComponentProps,
} from './content';

/** Renders a table block from `content.headers` and `content.rows`. */
export function TableBlock({ block, className }: BlockComponentProps): ReactElement {
  const content = blockContent(block);
  const headers = asStringArray(content.headers);
  const rows = asStringMatrix(content.rows);
  return (
    <div className={clsx('aurora-table-block', className)} data-block-id={block.id}>
      <table>
        {headers.length > 0 && (
          <thead>
            <tr>
              {headers.map((h, i) => (
                <th key={i}>{h}</th>
              ))}
            </tr>
          </thead>
        )}
        <tbody>
          {rows.map((row, ri) => (
            <tr key={ri}>
              {row.map((cell, ci) => (
                <td key={ci}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
