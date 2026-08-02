import { useState, type ReactElement } from 'react';
import clsx from 'clsx';

/** A navigation entry in the sidebar tree. */
export interface SidebarItem {
  id: string;
  label: string;
  /** Optional icon glyph/emoji. */
  icon?: string;
  /** Category of the entry (document, collection, project, ...). */
  kind?: string;
  /** Nested children. */
  children?: SidebarItem[];
}

export interface SidebarProps {
  items: SidebarItem[];
  onSelect?: (item: SidebarItem) => void;
  /** Initially collapsed. */
  defaultCollapsed?: boolean;
  className?: string;
}

/** Collapsible navigation sidebar rendering a workspace tree. */
export function Sidebar({
  items,
  onSelect,
  defaultCollapsed = false,
  className,
}: SidebarProps): ReactElement {
  const [collapsed, setCollapsed] = useState(defaultCollapsed);

  return (
    <nav
      className={clsx('aurora-sidebar', collapsed && 'collapsed', className)}
      aria-label="Workspace navigation"
    >
      <button
        type="button"
        className="aurora-sidebar-toggle"
        aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        aria-expanded={!collapsed}
        onClick={() => setCollapsed((c) => !c)}
      >
        {collapsed ? '»' : '«'}
      </button>
      <ul className="aurora-sidebar-list" role="tree">
        {items.map((item) => (
          <SidebarNode
            key={item.id}
            item={item}
            level={0}
            collapsed={collapsed}
            onSelect={onSelect}
          />
        ))}
      </ul>
    </nav>
  );
}

interface SidebarNodeProps {
  item: SidebarItem;
  level: number;
  collapsed: boolean;
  onSelect?: (item: SidebarItem) => void;
}

function SidebarNode({
  item,
  level,
  collapsed,
  onSelect,
}: SidebarNodeProps): ReactElement {
  const [expanded, setExpanded] = useState(true);
  const hasChildren = (item.children?.length ?? 0) > 0;

  const handleSelect = (): void => onSelect?.(item);

  return (
    <li role="treeitem" aria-expanded={hasChildren ? expanded : undefined}>
      <div
        className="aurora-sidebar-item"
        style={{ paddingLeft: `${level * 12 + 8}px` }}
      >
        {hasChildren && !collapsed ? (
          <button
            type="button"
            className="aurora-sidebar-expand"
            aria-label={expanded ? 'Collapse' : 'Expand'}
            onClick={() => setExpanded((e) => !e)}
          >
            {expanded ? '▾' : '▸'}
          </button>
        ) : null}
        <button
          type="button"
          className="aurora-sidebar-button"
          onClick={handleSelect}
          title={item.label}
        >
          {item.icon && <span className="aurora-sidebar-icon">{item.icon}</span>}
          {!collapsed && <span className="aurora-sidebar-label">{item.label}</span>}
        </button>
      </div>
      {hasChildren && expanded && !collapsed ? (
        <ul role="group">
          {item.children!.map((child) => (
            <SidebarNode
              key={child.id}
              item={child}
              level={level + 1}
              collapsed={collapsed}
              onSelect={onSelect}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
