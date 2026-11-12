/**
 * A-05（TA 2026-11-12 工单）: 页面面包屑 — parent_id 递归路径导航。
 *
 * - VM 字段双端共用: {id,title}[]（Rust 侧 core::block_ref::breadcrumb_of
 *   单一递归源——禁双端各写查询）
 * - 超长折叠: >4 级时中间级折叠为「…」（首尾各留 2——DoD 面）
 * - 无障碍: nav + aria-label="位置";每级 button 可聚焦回车
 */
import clsx from 'clsx';
import type { ReactElement } from 'react';

export interface BreadcrumbCrumb {
  id: string;
  title: string;
}

export interface NoteBreadcrumbProps {
  /** root→当前 有序链（含当前笔记——末级不可点）。 */
  crumbs: BreadcrumbCrumb[];
  /** 级跳转（当前级与占位级不触发）。 */
  onSelect?: (id: string) => void;
  className?: string;
}

/** 折叠阈值:超过 4 级启用中间折叠。 */
const COLLAPSE_OVER = 4;

/** 折叠后首/尾保留数。 */
const KEEP_HEAD = 2;
const KEEP_TAIL = 2;

export function NoteBreadcrumb(props: NoteBreadcrumbProps): ReactElement | null {
  const { crumbs, onSelect, className } = props;
  if (!crumbs || crumbs.length === 0) return null;

  const collapse = crumbs.length > COLLAPSE_OVER;
  const head = collapse ? crumbs.slice(0, KEEP_HEAD) : crumbs;
  const tail = collapse ? crumbs.slice(-KEEP_TAIL) : [];
  const hiddenCount = crumbs.length - head.length - tail.length;

  const item = (c: BreadcrumbCrumb, idx: number, isLast: boolean): ReactElement => (
    <span className="nb-item" key={c.id + idx}>
      {isLast ? (
        <span className="nb-current" aria-current="page">
          {c.title || '未命名'}
        </span>
      ) : (
        <button
          type="button"
          className="nb-link"
          onClick={() => onSelect?.(c.id)}
          aria-label={`跳转到 ${c.title || '未命名'}`}
        >
          {c.title || '未命名'}
        </button>
      )}
      {!isLast && <span className="nb-sep" aria-hidden="true">/</span>}
    </span>
  );

  return (
    <nav className={clsx('aurora-note-breadcrumb', className)} aria-label="位置">
      {head.map((c, i) => item(c, i, !collapse && i === crumbs.length - 1))}
      {collapse && (
        <>
          <span className="nb-item">
            <span className="nb-ellipsis" title={`${hiddenCount} 级已折叠`}>
              …
            </span>
            <span className="nb-sep" aria-hidden="true">/</span>
          </span>
          {tail.map((c, i) => item(c, i, i === tail.length - 1))}
        </>
      )}
    </nav>
  );
}
