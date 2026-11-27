/**
 * useSwipeRow — U-06: 触屏行滑动手势 hook（双方向阈值判定,shared 单源派生）。
 *
 * 背景: V26 前 TasksView 长按删/TodayView 左滑删/NotesView 双向滑各自内联实现,
 * 阈值 60/64 各異——U-06 收敛为单源 hook,F-04 在其上加「右滑=完成/左滑=改期」语义。
 *
 * 契约（不猜方向语义——语义由调用方注入,本 hook 只做物理判定）:
 * - left: 累计位移 dx < -threshold 触发（默认 64）
 * - right: dx > threshold 触发
 * - 同一触摸过程只触发一次（fired 锁——touchend 复位）
 * - 纵向位移 > 24 视为滚动,取消判定（列表滚动误触防线）
 * - 返回句柄 props 直接展开到行元素:onTouchStart/onTouchMove/onTouchEnd
 */
import { useRef, useCallback } from 'react';

export interface SwipeHandlers {
  onTouchStart: (e: React.TouchEvent) => void;
  onTouchMove: (e: React.TouchEvent) => void;
  onTouchEnd: (e: React.TouchEvent) => void;
}

export interface UseSwipeRowOptions {
  /** 触发阈值 px（默认 64——与 NotesView 既有值对齐） */
  threshold?: number;
  /** 左滑命中（dx 负向超阈）;返回 false 抑制 */
  onLeft?: () => boolean | void;
  /** 右滑命中（dx 正向超阈）;返回 false 抑制 */
  onRight?: () => boolean | void;
}

/** 行滑动手势（单行绑定——列表每行一个 hook 实例或复用 id 路由,见 TasksView/TodayView 用法）。 */
export function useSwipeRow(options: UseSwipeRowOptions = {}): SwipeHandlers {
  const { threshold = 64, onLeft, onRight } = options;
  const st = useRef<{ x: number; y: number; fired: boolean } | null>(null);

  const onTouchStart = useCallback((e: React.TouchEvent) => {
    const t = e.touches[0];
    st.current = { x: t.clientX, y: t.clientY, fired: false };
  }, []);

  const onTouchMove = useCallback((e: React.TouchEvent) => {
    const s = st.current;
    if (!s || s.fired) return;
    const t = e.touches[0];
    const dx = t.clientX - s.x;
    const dy = t.clientY - s.y;
    // 纵向滚动优先（防列表滚动误触）
    if (Math.abs(dy) > 24) { s.fired = true; return; }
    if (dx < -threshold && onLeft) {
      if (onLeft() !== false) s.fired = true;
    } else if (dx > threshold && onRight) {
      if (onRight() !== false) s.fired = true;
    }
  }, [threshold, onLeft, onRight]);

  const onTouchEnd = useCallback(() => {
    st.current = null;
  }, []);

  return { onTouchStart, onTouchMove, onTouchEnd };
}
