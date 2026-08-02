import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { DocumentEditor } from '../editors';
import { block, doc } from './fixtures';

describe('DocumentEditor', () => {
  it('mounts and renders the formatting toolbar', async () => {
    render(<DocumentEditor document={doc([block('text', { text: 'Hello world' })])} />);
    expect(screen.getByRole('toolbar')).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'bold' })).not.toBeDisabled();
    });
    for (const name of ['bold', 'italic', 'heading', 'code', 'list', 'quote']) {
      expect(screen.getByRole('button', { name })).toBeInTheDocument();
    }
  });

  it('renders the initial document content', async () => {
    render(<DocumentEditor document={doc([block('text', { text: 'Hello world' })])} />);
    expect(await screen.findByText('Hello world')).toBeInTheDocument();
  });

  it('emits an updated document when a toolbar command runs', async () => {
    const onChange = vi.fn();
    render(
      <DocumentEditor
        document={doc([block('text', { text: 'Hello world' })])}
        onChange={onChange}
      />,
    );
    const headingBtn = await screen.findByRole('button', { name: 'heading' });
    await waitFor(() => expect(headingBtn).not.toBeDisabled());
    fireEvent.click(headingBtn);
    await waitFor(() => expect(onChange).toHaveBeenCalled());
    const emitted = onChange.mock.calls[0][0];
    expect(emitted.blocks.length).toBeGreaterThan(0);
    expect(emitted.version).toBeGreaterThan(1);
  });
});
