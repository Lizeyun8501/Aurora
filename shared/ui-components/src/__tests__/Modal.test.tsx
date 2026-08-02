import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Modal } from '../layouts';

describe('Modal', () => {
  it('renders children when open', () => {
    render(
      <Modal open title="My Modal">
        <p>Modal body</p>
      </Modal>,
    );
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('Modal body')).toBeInTheDocument();
    expect(screen.getByText('My Modal')).toBeInTheDocument();
  });

  it('renders nothing when closed', () => {
    render(
      <Modal open={false} title="My Modal">
        <p>Modal body</p>
      </Modal>,
    );
    expect(screen.queryByText('Modal body')).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('calls onClose when Escape is pressed', () => {
    const onClose = vi.fn();
    render(
      <Modal open onClose={onClose}>
        <p>Body</p>
      </Modal>,
    );
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('calls onClose when the backdrop is clicked', () => {
    const onClose = vi.fn();
    render(
      <Modal open onClose={onClose}>
        <p>Body</p>
      </Modal>,
    );
    const backdrop = document.querySelector('.aurora-modal-backdrop') as HTMLElement;
    expect(backdrop).toBeInTheDocument();
    fireEvent.mouseDown(backdrop);
    expect(onClose).toHaveBeenCalled();
  });

  it('calls onClose via the close button', () => {
    const onClose = vi.fn();
    render(
      <Modal open title="T" onClose={onClose}>
        <p>Body</p>
      </Modal>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    expect(onClose).toHaveBeenCalled();
  });
});
