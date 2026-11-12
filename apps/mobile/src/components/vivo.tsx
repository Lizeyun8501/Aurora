/**
 * vivo 小件 — B-02 切片三: VivoIcon/FocusBadge 从 MobileApp 随 TodayView 迁出。
 * （IconDef 类型随迁——MobileApp 内 IconDef 声明保留,此处独立声明零耦合）。
 */
import React, { useMemo } from 'react';
import { platform } from '../adapters/androidPlatform';
import type { Task, IconDef } from './primitives';
import { focusSummary } from './focusSummary';


/** VIVO 图标块: 圆角方底色+白字形（16-22px 视觉等效 emoji 但可控配色） */
export function VivoIcon({ def, size = 22 }: { def: IconDef; size?: number }) {
  return (
    <span className="vivo-icon" aria-label={def.label} title={def.label}
      style={{ width: size, height: size, background: def.color, fontSize: Math.round(size * 0.58) }}>
      {def.glyph}
    </span>
  );
}

/** V26 M2/DK-06: 预计 vs 实际徽标 — 偏差率着色（|d|>0.3 警示）。
 *  FFI 周回顾汇总优先; localStorage 演示级降级; 双零隐藏。 */
export function FocusBadge({ tasks }: { tasks: Task[] }) {
  // V26 M2/DK-06: Rust 投影真源优先（Android 桥），FFI 不可用降级本地推导
  const remote = useMemo(() => platform.todayFocusSummary(), []);
  const { estimate, actual, deviation } = remote ?? focusSummary(tasks);
  if (estimate === 0 && actual === 0) return null;
  const devText = deviation === null ? '' :
    deviation === 0 ? '（持平）' : '（' + (deviation > 0 ? '+' : '') + Math.round(deviation * 100) + '%）';
  const warn = deviation !== null && Math.abs(deviation) > 0.3;
  return (
    <span
      className="focus-badge"
      title="预计 vs 实际（周回顾）"
      style={{ color: warn ? 'var(--warning, #b45309)' : 'var(--text-tertiary)' }}
    >
      预计 {estimate}′ · 实际 {actual}′{devText}
    </span>
  );
}
