/**
 * DK-15c：marketplace 徽章 UI——能力面数据单源映射（纯函数）。
 *
 * 与 Rust Capability 对齐（15a 对齐测试的 TS 侧消费方）：
 * shared/types/src/plugin.ts Capability 枚举（fs/network/native/storage/
 * log/clipboard/notifications/dialog）——变体漂移即 fail-closed 显示。
 * user-output 为 15b 预置项（Capability::UserOutput 落地后点亮）。
 */

export interface BadgeSpec {
  key: string;
  label: string;
  /** 危险能力（fs/network/native）警示色语义 */
  danger: boolean;
}

/** 徽章文案/颜色映射单源（展示层 fail-closed：未知变体→"unknown"） */
export const BADGE_MAP: Record<string, Omit<BadgeSpec, 'key'>> = {
  fs: { label: '文件系统', danger: true },
  network: { label: '网络', danger: true },
  native: { label: '原生宿主', danger: true },
  storage: { label: '文档存储', danger: false },
  log: { label: '日志', danger: false },
  'user-output': { label: '用户输出', danger: false },
  clipboard: { label: '剪贴板', danger: true },
  notifications: { label: '系统通知', danger: false },
  dialog: { label: '模态对话框', danger: false },
};

/**
 * granted 集合→徽章序列（逐枚举，保持传入顺序去重）。
 * 未授予能力不渲染（详情页只展示已获面）——零权限插件授予空集，
 * 由 callers 以 "零权限" 徽章正展示（默认零权限是卖点）。
 */
/**
 * accept_share（DK-17 L2）→ 中性色徽章（非能力项，数据流偏好）。
 * true 时插件详情页显示"可接收分享"——与能力徽章分开渲染。
 */
export function acceptShareBadge(acceptShare: boolean | undefined): BadgeSpec | null {
  if (!acceptShare) return null;
  return { key: 'accept-share', label: '可接收分享', danger: false };
}

export function badgesForGranted(granted: readonly string[]): BadgeSpec[] {
  const seen = new Set<string>();
  const out: BadgeSpec[] = [];
  for (const g of granted) {
    if (seen.has(g)) continue;
    seen.add(g);
    const m = BADGE_MAP[g];
    // fail-closed 视觉层：未知变体显示 "unknown"（不静默丢弃）
    out.push(m
      ? { key: g, label: m.label, danger: m.danger }
      : { key: g, label: 'unknown', danger: true });
  }
  return out;
}
