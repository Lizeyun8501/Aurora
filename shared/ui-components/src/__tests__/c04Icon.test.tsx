// C-04: 图标单一实现+字段一致性行为级测试（桌面面包屑折叠形态由
// a05BlockRefBreadcrumb 既有套覆盖——此处补 NoteIcon 面与 CoverHeader 关键语义）。
import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { VivoIcon, getIconDef, setIconDef, ALL_ICONS, BOOK_ICONS, DOC_ICONS } from '../views/NoteIcon';

describe('C-04 NoteIcon 单一实现', () => {
  it('getIconDef: 无覆盖回默认档（book/doc 分档）', () => {
    expect(getIconDef('book', 'c04-none').id).toBe(BOOK_ICONS[0].id);
    expect(getIconDef('doc', 'c04-none').id).toBe(DOC_ICONS[0].id);
  });

  it('get/setIconDef: aurora.icon.<id> 字段往返（三消费面同源前提）', () => {
    setIconDef('c04-x', 'doc-star');
    expect(getIconDef('doc', 'c04-x').id).toBe('doc-star');
    expect(getIconDef('book', 'c04-x').id).toBe('doc-star'); // 同字段不分类读
    localStorage.removeItem('aurora.icon.c04-x');
  });

  it('VivoIcon: aria-label+title 双载体（无障碍）', () => {
    const { container } = render(<VivoIcon def={DOC_ICONS[2]} />);
    const el = container.querySelector('.vivo-icon') as HTMLElement;
    expect(el.getAttribute('aria-label')).toBe('星标');
    expect(el.getAttribute('title')).toBe('星标');
  });

  it('ALL_ICONS: 16 def 无重复 id（选择器面板完整性）', () => {
    const ids = ALL_ICONS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.length).toBe(16);
  });
});
