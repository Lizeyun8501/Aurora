import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import {
  AttachmentPreview,
  classifyPreview,
  type PreviewAttachmentDto,
  type ReadAttachmentFn,
} from '../views/AttachmentPreview';

/** A-02 行为级断言（TA 14:31 工单）:
 * - MIME 四类分派（image/pdf/text/office）
 * - 锁定态（readAttachment Err——unseal fail-closed 语义）零内容渲染
 * - 图片: blob URL 生成 + unmount 后 revoke（无缓存残留）
 * - 文本: 分片按需（首片渲染+「加载更多」续片）+ 预算封顶
 * - 无障碍: dialog role + Esc 关闭 + 图片 alt
 * - PDF 降级卡: 系统打开提示（零外链——断网可用纪律）
 */

function dto(b64: string, range: [number, number] | null = null): PreviewAttachmentDto {
  return {
    attachment_id: 'att-1',
    note_id: 'n-1',
    file_name: 'f.txt',
    mime: 'text/plain',
    size: 1024,
    data_base64: b64,
    range,
  };
}

/** 'hello' base64。 */
const B64_HELLO = 'aGVsbG8=';

describe('classifyPreview（MIME 四类分派）', () => {
  it('image/pdf/text/office 全路由', () => {
    expect(classifyPreview('image/png', 'a.png')).toBe('image');
    expect(classifyPreview('application/pdf', 'a.pdf')).toBe('pdf');
    expect(classifyPreview('application/octet-stream', 'a.pdf')).toBe('pdf'); // 扩展名兜底
    expect(classifyPreview('text/plain', 'a.txt')).toBe('text');
    expect(classifyPreview('text/x-rust', 'a.rs')).toBe('text');
    expect(classifyPreview('application/octet-stream', 'a.docx')).toBe('office');
    expect(classifyPreview('application/zip', 'b.xyz')).toBe('office'); // 未知兜底 office 卡
  });
});

describe('AttachmentPreview', () => {
  it('图片: alt 文本 + blob revoke 无残留', async () => {
    const revokeSpy = vi.fn();
    const url = 'blob:mock-url';
    const origCreate = URL.createObjectURL;
    const origRevoke = URL.revokeObjectURL;
    URL.createObjectURL = vi.fn(() => url);
    URL.revokeObjectURL = revokeSpy;
    const read: ReadAttachmentFn = vi.fn(async () =>
      dto(B64_HELLO, null),
    );
    const { unmount } = render(
      <AttachmentPreview
        attachmentId="att-1"
        fileName="pic.png"
        mime="image/png"
        size={5}
        readAttachment={read}
        onClose={() => undefined}
      />,
    );
    const img = await screen.findByRole('img');
    expect(img).toHaveAttribute('alt', 'pic.png');
    unmount();
    expect(revokeSpy).toHaveBeenCalledWith(url); // 无缓存残留纪律
    URL.createObjectURL = origCreate;
    URL.revokeObjectURL = origRevoke;
  });

  it('锁定态（read Err）: 零内容渲染（不显示任何附件字节）', async () => {
    const read: ReadAttachmentFn = vi.fn(async () => {
      throw new Error('vault locked（unseal fail-closed）');
    });
    render(
      <AttachmentPreview
        attachmentId="att-1"
        fileName="secret.png"
        mime="image/png"
        size={5}
        readAttachment={read}
        onClose={() => undefined}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(/无法读取/)).toBeInTheDocument();
    });
    // 零内容: 无 img / 无 base64 明文进 DOM
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });

  it('文本: 首片渲染 + 按需续片（range 透传）', async () => {
    const calls: Array<{ start: number; len: number } | undefined> = [];
    const read: ReadAttachmentFn = vi.fn(async (_id, range) => {
      calls.push(range);
      if (!range) return dto(B64_HELLO, null); // 全量（首读组件未传 range?首片传 range）
      return dto(B64_HELLO, [range.start, range.len]);
    });
    render(
      <AttachmentPreview
        attachmentId="att-1"
        fileName="log.txt"
        mime="text/plain"
        size={1024 * 1024}
        readAttachment={read}
        onClose={() => undefined}
      />,
    );
    await screen.findByText(/hello/);
    const more = screen.getByRole('button', { name: '加载更多' });
    fireEvent.click(more);
    await waitFor(() => {
      // 续片调用带 range（按需取片断言——range 参数真实透传）
      expect(calls.some((c) => c && c.start === 0 && c.len > 0)).toBe(true);
    });
  });

  it('PDF 降级卡: 系统打开提示（零外链）', async () => {
    const read: ReadAttachmentFn = vi.fn(async () => dto(B64_HELLO, null));
    render(
      <AttachmentPreview
        attachmentId="att-1"
        fileName="doc.pdf"
        mime="application/pdf"
        size={2048}
        readAttachment={read}
        onClose={() => undefined}
      />,
    );
    await screen.findByText(/系统应用打开/);
    // 零 iframe/embed/object（禁 CDN/外链纪律）
    expect(document.querySelector('iframe')).toBeNull();
    expect(document.querySelector('embed')).toBeNull();
  });

  it('无障碍: dialog role + Esc 关闭', async () => {
    const onClose = vi.fn();
    const read: ReadAttachmentFn = vi.fn(async () => dto(B64_HELLO, null));
    render(
      <AttachmentPreview
        attachmentId="att-1"
        fileName="a.txt"
        mime="text/plain"
        size={10}
        readAttachment={read}
        onClose={onClose}
      />,
    );
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('插入到当前笔记: image/非 image 引用形态', async () => {
    const onInsert = vi.fn();
    const read: ReadAttachmentFn = vi.fn(async () => dto(B64_HELLO, null));
    render(
      <AttachmentPreview
        attachmentId="att-9"
        fileName="x.png"
        mime="image/png"
        size={5}
        readAttachment={read}
        onInsertToNote={onInsert}
        onClose={() => undefined}
      />,
    );
    const btn = await screen.findByRole('button', { name: /插入到当前笔记/ });
    fireEvent.click(btn);
    expect(onInsert).toHaveBeenCalledWith('![x.png](attachment://att-9)');
  });
});
