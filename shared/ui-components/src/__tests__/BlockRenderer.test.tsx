import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { BlockRenderer } from '../blocks';
import { block } from './fixtures';

describe('BlockRenderer', () => {
  it('renders a text block', () => {
    const { container } = render(<BlockRenderer block={block('text', { text: 'Hello world' })} />);
    expect(screen.getByText('Hello world')).toBeInTheDocument();
    expect(container.querySelector('[data-block-id]')).not.toBeNull();
  });

  it('renders a heading block with the correct level', () => {
    render(<BlockRenderer block={block('heading', { text: 'Title', level: 3 })} />);
    const heading = screen.getByText('Title');
    expect(heading.tagName).toBe('H3');
  });

  it('falls back to h1 for an invalid heading level', () => {
    render(<BlockRenderer block={block('heading', { text: 'Title', level: 99 })} />);
    expect(screen.getByText('Title').tagName).toBe('H1');
  });

  it('renders a code block with a language', () => {
    const { container } = render(
      <BlockRenderer block={block('code', { text: 'const x = 1', language: 'ts' })} />,
    );
    const pre = container.querySelector('pre');
    expect(pre).not.toBeNull();
    expect(pre).toHaveAttribute('data-language', 'ts');
    expect(screen.getByText('const x = 1')).toBeInTheDocument();
  });

  it('renders an image block', () => {
    const { container } = render(
      <BlockRenderer block={block('image', { url: 'https://x.png', alt: 'pic' })} />,
    );
    const img = container.querySelector('img');
    expect(img).not.toBeNull();
    expect(img).toHaveAttribute('src', 'https://x.png');
    expect(img).toHaveAttribute('alt', 'pic');
  });

  it('renders a placeholder when an image has no url', () => {
    render(<BlockRenderer block={block('image', {})} />);
    expect(screen.getByText('No image source')).toBeInTheDocument();
  });

  it('renders a table block', () => {
    const { container } = render(
      <BlockRenderer block={block('table', { headers: ['A', 'B'], rows: [['1', '2']] })} />,
    );
    expect(container.querySelectorAll('th')).toHaveLength(2);
    expect(container.querySelectorAll('td')).toHaveLength(2);
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('renders a divider block', () => {
    const { container } = render(<BlockRenderer block={block('divider')} />);
    expect(container.querySelector('hr')).not.toBeNull();
  });

  it('renders a quote block', () => {
    const { container } = render(
      <BlockRenderer block={block('quote', { text: 'quoted', cite: 'src' })} />,
    );
    const bq = container.querySelector('blockquote');
    expect(bq).not.toBeNull();
    expect(bq).toHaveAttribute('cite', 'src');
    expect(screen.getByText('quoted')).toBeInTheDocument();
  });

  it('renders a list item block', () => {
    render(<BlockRenderer block={block('list_item', { text: 'an item', ordered: false })} />);
    expect(screen.getByText('an item')).toBeInTheDocument();
  });

  it('renders a todo item block reflecting checked state', () => {
    const { container } = render(
      <BlockRenderer block={block('todo_item', { text: 'done task', checked: true })} />,
    );
    const checkbox = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(checkbox).not.toBeNull();
    expect(checkbox).toBeChecked();
  });

  it('renders an unsupported fallback for unknown block types', () => {
    render(<BlockRenderer block={block('custom:foo', {})} />);
    expect(screen.getByText(/Unsupported block type/)).toBeInTheDocument();
  });

  it('uses a custom renderer from the registry when provided', () => {
    render(
      <BlockRenderer
        block={block('custom:foo', {})}
        blockRenderers={{ 'custom:foo': () => <div>custom-rendered</div> }}
      />,
    );
    expect(screen.getByText('custom-rendered')).toBeInTheDocument();
  });

  it('renders child blocks recursively', () => {
    const parent = block('text', { text: 'parent' }, {
      children: [block('text', { text: 'child' })],
    });
    render(<BlockRenderer block={parent} />);
    expect(screen.getByText('parent')).toBeInTheDocument();
    expect(screen.getByText('child')).toBeInTheDocument();
  });
});
