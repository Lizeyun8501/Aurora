import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { NoteBreadcrumb } from '../views/NoteBreadcrumb';
import { auroraSchema } from '../schema/auroraSchema';
import { mdToNodes, docToMd } from '../editors/auroraMarkdown';
import type { Node as ProseNode } from 'prosemirror-model';

/**
 * A-05（TA 2026-11-12 工单）行为级断言:
 * - 面包屑: nav+aria-label="位置" / 4 级以上中间折叠「…」 / 各级可点 / 末级 aria-current
 * - 块引用 md 往返: ((id)) / ((id#anchor)) → block_ref 节点（noteId/anchor attrs）→ 发射还原
 * - 混排: 块引用与表格/任务/公式同文档往返不丢型
 */

describe('NoteBreadcrumb', () => {
  it('nav 无障碍语义 + 末级 aria-current', () => {
    render(
      <NoteBreadcrumb
        crumbs={[
          { id: 'a', title: '一级' },
          { id: 'b', title: '二级' },
          { id: 'c', title: '当前' },
        ]}
        onSelect={() => undefined}
      />,
    );
    expect(screen.getByRole('navigation', { name: '位置' })).toBeInTheDocument();
    expect(screen.getByText('当前').getAttribute('aria-current')).toBe('page');
  });

  it('各级可点击（跳转回调真实触发）', () => {
    const clicked: string[] = [];
    render(
      <NoteBreadcrumb
        crumbs={[
          { id: 'a', title: '一级' },
          { id: 'b', title: '当前' },
        ]}
        onSelect={(id) => clicked.push(id)}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: '跳转到 一级' }));
    expect(clicked).toEqual(['a']);
  });

  it('5 级折叠: 首尾各 2 + 中间「…」（hidden 计数 title）', () => {
    render(
      <NoteBreadcrumb
        crumbs={[
          { id: '1', title: 'L1' },
          { id: '2', title: 'L2' },
          { id: '3', title: 'L3' },
          { id: '4', title: 'L4' },
          { id: '5', title: 'L5' },
        ]}
        onSelect={() => undefined}
      />,
    );
    expect(screen.getByText('L1')).toBeInTheDocument();
    expect(screen.getByText('L2')).toBeInTheDocument();
    expect(screen.getByText('L4')).toBeInTheDocument();
    expect(screen.getByText('L5')).toBeInTheDocument();
    expect(screen.queryByText('L3')).not.toBeInTheDocument();
    expect(screen.getByTitle('1 级已折叠')).toBeInTheDocument();
  });

  it('4 级不折叠（阈值边界）', () => {
    render(
      <NoteBreadcrumb
        crumbs={[
          { id: '1', title: 'L1' },
          { id: '2', title: 'L2' },
          { id: '3', title: 'L3' },
          { id: '4', title: 'L4' },
        ]}
        onSelect={() => undefined}
      />,
    );
    expect(screen.getByText('L3')).toBeInTheDocument();
    expect(screen.queryByTitle(/已折叠/)).not.toBeInTheDocument();
  });
});

describe('A-05 块引用 md 往返', () => {
  it('((id)) → block_ref(noteId,anchor=null) → 发射还原', () => {
    const nodes = mdToNodes(auroraSchema, '引言段\n\n((note-abc))\n\n结尾段');
    const ref = nodes.find((n) => n.type.name === 'block_ref') as ProseNode | undefined;
    expect(ref).toBeDefined();
    expect(ref!.attrs.noteId).toBe('note-abc');
    expect(ref!.attrs.anchor).toBeNull();
    // 往返发射
    const doc = auroraSchema.nodes.doc.create(null, nodes as ProseNode[]);
    const md = docToMd(doc);
    expect(md).toContain('((note-abc))');
    // 再解析回——attrs 保真
    const back = mdToNodes(auroraSchema, md).find((n) => n.type.name === 'block_ref') as ProseNode;
    expect(back.attrs.noteId).toBe('note-abc');
    expect(back.attrs.anchor).toBeNull();
  });

  it('((id#锚点)) anchor 解析+往返', () => {
    const nodes = mdToNodes(auroraSchema, '((note-x#实现细节))');
    const ref = nodes[0] as ProseNode;
    expect(ref.type.name).toBe('block_ref');
    expect(ref.attrs.noteId).toBe('note-x');
    expect(ref.attrs.anchor).toBe('实现细节');
    const md = docToMd(auroraSchema.nodes.doc.create(null, nodes as ProseNode[]));
    expect(md).toBe('((note-x#实现细节))');
  });

  it('混排: 块引用+表格+任务+公式同文档不丢型', () => {
    const md = [
      '| a | b |',
      '| --- | --- |',
      '| 1 | 2 |',
      '',
      '((src-note#章节一))',
      '',
      '- [x] 已完成',
      '',
      '$$E=mc^2$$',
    ].join('\n');
    const types = mdToNodes(auroraSchema, md).map((n) => n.type.name);
    expect(types).toContain('table');
    expect(types).toContain('block_ref');
    expect(types).toContain('task_block');
    expect(types).toContain('math_block');
  });
});
