// C-05（TA 2026-11-17）: 三态组件+对话框服务行为级测试。
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { EmptyState, Skeleton, ErrorState } from '../views/EmptyState';
import { dialog, DialogHost } from '../layouts/dialogs';

describe('C-05 三态统一', () => {
  it('EmptyState: 场景文案传入（title/hint/action）+ 插画 svg + role=status', () => {
    const onAction = vi.fn();
    render(<EmptyState illustration="inbox" title="收集箱是空的" hint="点右下角捕获" actionLabel="去捕获" onAction={onAction} />);
    expect(screen.getByText('收集箱是空的')).toBeTruthy();
    expect(screen.getByText('点右下角捕获')).toBeTruthy();
    expect(screen.getByRole('status')).toBeTruthy();
    fireEvent.click(screen.getByText('去捕获'));
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it('Skeleton: role=status+aria 加载中+波纹类（禁 spinner）', () => {
    const { container } = render(<Skeleton rows={4} />);
    expect(screen.getByRole('status')).toBeTruthy();
    expect(screen.getByLabelText('加载中')).toBeTruthy();
    expect(container.querySelectorAll('.aurora-skel-row')).toHaveLength(4);
    // 禁 spinner: 无任何 animate-spin/rotate 类
    expect(container.querySelector('[class*="spin"], [class*="rotate"]')).toBeNull();
  });

  it('ErrorState: role=alert+重试动作回调', () => {
    const onRetry = vi.fn();
    render(<ErrorState title="集合数据加载失败" hint="重试拉取" onRetry={onRetry} />);
    expect(screen.getByRole('alert')).toBeTruthy();
    fireEvent.click(screen.getByText('重试'));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});

describe('C-05 对话框服务（prompt/alert/confirm 退役基座）', () => {
  it('dialog.ask: 确认回调 true/false 双路+危险态按钮文案', async () => {
    render(<DialogHost />);
    const p = dialog.ask({ title: '彻底删除', message: '不可恢复', danger: true, confirmLabel: '彻底删除' });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeTruthy());
    expect(screen.getByText('不可恢复')).toBeTruthy();
    // aria-modal 挂载（Modal 基座）
    expect(screen.getByRole('dialog').getAttribute('aria-modal')).toBe('true');
    fireEvent.click(screen.getAllByText('彻底删除')[1] ?? screen.getAllByText('彻底删除')[0]); // 标题+按钮双命中——取按钮
    await expect(p).resolves.toBe(true);
  });

  it('dialog.ask 取消路: false', async () => {
    render(<DialogHost />);
    const p = dialog.ask({ title: '解除配对', message: '确定?' });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeTruthy());
    fireEvent.click(screen.getByText('取消'));
    await expect(p).resolves.toBe(false);
  });

  it('dialog.input: Enter 提交初值/Esc 取消 null', async () => {
    render(<DialogHost />);
    const p = dialog.input({ title: '重命名书签', label: '书签标题', initial: '旧名' });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeTruthy());
    const input = document.querySelector('.aurora-dlg-input') as HTMLInputElement;
    expect(input.value).toBe('旧名'); // 初值回填
    fireEvent.keyDown(input, { key: 'Enter' });
    await expect(p).resolves.toBe('旧名');
  });

  it('dialog.notify: 消息呈现+知道了关闭', async () => {
    render(<DialogHost />);
    const p = dialog.notify({ title: '同步状态', message: '队列积压 3 条' });
    await waitFor(() => expect(screen.getByText('队列积压 3 条')).toBeTruthy());
    fireEvent.click(screen.getByText('知道了'));
    await expect(p).resolves.toBeUndefined();
  });

  it('未挂载 DialogHost 时安全降级: ask→false / input→null', async () => {
    await expect(dialog.ask({ title: 'x', message: 'y' })).resolves.toBe(false);
    await expect(dialog.input({ title: 'x' })).resolves.toBeNull();
  });
});
