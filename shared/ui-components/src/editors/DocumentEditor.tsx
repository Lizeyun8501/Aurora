import {
  useEffect,
  useReducer,
  useRef,
  useState,
  type ReactElement,
} from 'react';
import { EditorContent, useEditor } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import type { Block, BlockType, Document, JsonValue } from '@aurora/shared-types';
import clsx from 'clsx';
import { asNumber, asString, blockContent } from '../blocks/content';

/* -------------------------------------------------------------------------- */
/* Block <-> HTML serialization (simplified; TipTap owns the live doc state). */
/* -------------------------------------------------------------------------- */

let blockIdCounter = 0;
function nextBlockId(): string {
  blockIdCounter += 1;
  return `editor-block-${blockIdCounter}`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function blockToHtml(block: Block): string {
  const text = asString(blockContent(block).text);
  switch (block.block_type) {
    case 'heading': {
      const level = clampLevel(asNumber(blockContent(block).level) || 1);
      return `<h${level}>${escapeHtml(text)}</h${level}>`;
    }
    case 'code':
      return `<pre><code>${escapeHtml(text)}</code></pre>`;
    case 'quote':
      return `<blockquote>${escapeHtml(text)}</blockquote>`;
    case 'list_item':
      return `<ul><li>${escapeHtml(text)}</li></ul>`;
    case 'divider':
      return '<hr>';
    case 'todo_item':
    case 'image':
    case 'table':
    case 'text':
    default:
      return `<p>${escapeHtml(text)}</p>`;
  }
}

function blocksToHtml(blocks: Block[]): string {
  return blocks.map(blockToHtml).join('');
}

function clampLevel(n: number): 1 | 2 | 3 | 4 | 5 | 6 {
  if (n >= 1 && n <= 6) return n as 1 | 2 | 3 | 4 | 5 | 6;
  return 1;
}

function makeBlock(
  blockType: BlockType,
  content: JsonValue,
  now: string,
): Block {
  return {
    id: nextBlockId(),
    block_type: blockType,
    content,
    properties: {},
    children: [],
    created_at: now,
    updated_at: now,
  };
}

function nodeToBlock(node: Node, now: string): Block | null {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = (node.textContent ?? '').trim();
    if (!text) return null;
    return makeBlock('text', { text }, now);
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return null;
  const el = node as Element;
  const tag = el.tagName.toLowerCase();
  const text = el.textContent ?? '';
  switch (tag) {
    case 'h1':
    case 'h2':
    case 'h3':
    case 'h4':
    case 'h5':
    case 'h6':
      return makeBlock('heading', { text, level: Number(tag.slice(1)) }, now);
    case 'pre':
      return makeBlock('code', { text, language: 'plaintext' }, now);
    case 'blockquote':
      return makeBlock('quote', { text }, now);
    case 'li':
      return makeBlock('list_item', { text, ordered: false }, now);
    case 'hr':
      return makeBlock('divider', {}, now);
    case 'p':
    default:
      return makeBlock('text', { text }, now);
  }
}

function htmlToBlocks(html: string): Block[] {
  if (typeof document === 'undefined') return [];
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  const blocks: Block[] = [];
  parsed.body.childNodes.forEach((node) => {
    const block = nodeToBlock(node, new Date().toISOString());
    if (block) blocks.push(block);
  });
  return blocks;
}

/* -------------------------------------------------------------------------- */
/* Toolbar                                                                    */
/* -------------------------------------------------------------------------- */

interface ToolbarButtonDef {
  label: string;
  name: string;
  active: boolean;
  onClick: () => void;
}

/* -------------------------------------------------------------------------- */
/* DocumentEditor                                                             */
/* -------------------------------------------------------------------------- */

export interface DocumentEditorProps {
  document: Document;
  onChange?: (doc: Document) => void;
  className?: string;
}

/**
 * Main document editor wrapping TipTap (StarterKit). Initializes the editor
 * from the supplied `Document`'s blocks, emits an updated `Document` via
 * `onChange` on every editor transaction, and renders a formatting toolbar
 * (bold, italic, heading, code, list, quote).
 */
export function DocumentEditor({
  document: doc,
  onChange,
  className,
}: DocumentEditorProps): ReactElement {
  const [initialHtml] = useState(() => blocksToHtml(doc.blocks));

  // Refs avoid stale closures inside TipTap's `onUpdate`.
  const docRef = useRef(doc);
  docRef.current = doc;
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const emit = (html: string): void => {
    const current = docRef.current;
    const next: Document = {
      ...current,
      blocks: htmlToBlocks(html),
      updated_at: new Date().toISOString(),
      version: current.version + 1,
    };
    onChangeRef.current?.(next);
  };

  const editor = useEditor({
    extensions: [StarterKit],
    content: initialHtml,
    onUpdate: ({ editor }) => emit(editor.getHTML()),
  });

  // Re-render on selection/transaction so toolbar active-states stay fresh.
  const [, forceRender] = useReducer((x: number) => x + 1, 0);
  useEffect(() => {
    if (!editor) return;
    editor.on('transaction', forceRender);
    return () => {
      editor.off('transaction', forceRender);
    };
  }, [editor]);

  const buttons: ToolbarButtonDef[] = [
    {
      label: 'B',
      name: 'bold',
      active: editor?.isActive('bold') ?? false,
      onClick: () => editor?.chain().focus().toggleBold().run(),
    },
    {
      label: 'I',
      name: 'italic',
      active: editor?.isActive('italic') ?? false,
      onClick: () => editor?.chain().focus().toggleItalic().run(),
    },
    {
      label: 'H',
      name: 'heading',
      active: editor?.isActive('heading', { level: 2 }) ?? false,
      onClick: () => editor?.chain().focus().toggleHeading({ level: 2 }).run(),
    },
    {
      label: '<>',
      name: 'code',
      active: editor?.isActive('code') ?? false,
      onClick: () => editor?.chain().focus().toggleCode().run(),
    },
    {
      label: '• List',
      name: 'list',
      active: editor?.isActive('bulletList') ?? false,
      onClick: () => editor?.chain().focus().toggleBulletList().run(),
    },
    {
      label: '❝ Quote',
      name: 'quote',
      active: editor?.isActive('blockquote') ?? false,
      onClick: () => editor?.chain().focus().toggleBlockquote().run(),
    },
  ];

  return (
    <div className={clsx('aurora-document-editor', className)}>
      <div className="aurora-document-editor-toolbar" role="toolbar" aria-label="Formatting">
        {buttons.map((b) => (
          <button
            key={b.name}
            type="button"
            aria-label={b.name}
            aria-pressed={b.active}
            className={clsx('aurora-toolbar-button', b.active && 'active')}
            disabled={!editor}
            onClick={b.onClick}
          >
            {b.label}
          </button>
        ))}
      </div>
      <EditorContent editor={editor} className="aurora-document-editor-content" />
    </div>
  );
}
