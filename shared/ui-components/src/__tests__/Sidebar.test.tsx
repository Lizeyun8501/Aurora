import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Sidebar } from '../layouts';
import type { SidebarItem } from '../layouts';

const items: SidebarItem[] = [
  { id: 'd1', label: 'Doc One', icon: '📄', kind: 'document' },
  {
    id: 'c1',
    label: 'Collection',
    kind: 'collection',
    children: [{ id: 'd2', label: 'Nested Doc', kind: 'document' }],
  },
  { id: 'p1', label: 'Project', kind: 'project' },
];

describe('Sidebar', () => {
  it('renders top-level items', () => {
    render(<Sidebar items={items} />);
    expect(screen.getByText('Doc One')).toBeInTheDocument();
    expect(screen.getByText('Collection')).toBeInTheDocument();
    expect(screen.getByText('Project')).toBeInTheDocument();
  });

  it('renders nested children', () => {
    render(<Sidebar items={items} />);
    expect(screen.getByText('Nested Doc')).toBeInTheDocument();
  });

  it('calls onSelect with the clicked item', () => {
    const onSelect = vi.fn();
    render(<Sidebar items={items} onSelect={onSelect} />);
    fireEvent.click(screen.getByText('Doc One'));
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: 'd1' }));
  });

  it('collapses and hides labels', () => {
    render(<Sidebar items={items} />);
    const toggle = screen.getByRole('button', { name: 'Collapse sidebar' });
    fireEvent.click(toggle);
    expect(screen.queryByText('Doc One')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeInTheDocument();
  });
});
