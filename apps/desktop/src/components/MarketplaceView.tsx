/**
 * DK-15c：marketplace 徽章视图（插件详情能力面）。
 *
 * 勘察结论（15c 工单 5 行）：
 * 1. DesktopShell 用 MainView 枚举路由（'notes'|'today'|...），无 react-router
 * 2. marketplace UI 零既有实现——本组件新建
 * 3. 后端 crates/aurora-plugin/src/marketplace.rs 已就绪（PluginListing/sign）
 * 4. shared/types plugin.ts Capability/PermissionState 类型就绪（15a 已对齐收紧）
 * 5. 本卡仅详情徽章面——权限切换 UI 另卡、列表页不做（工单停手）
 *
 * 纯前端 diff（src/ 内，src-tauri 不动）。
 */
import type { Capability } from '@aurora/shared-types/plugin';
import { badgesForGranted } from './capabilityBadge';

export interface MarketplaceViewProps {
  /** 插件展示名（无后端 listing 拉取前由调用方传入） */
  pluginName: string;
  /** manifest granted 集合（Rust Capability 字符串——15a 类型对齐的 TS 消费） */
  granted: readonly Capability[];
}

/** 零权限徽章文案（默认零权限是卖点，正着展示） */
const ZERO_GRANT_LABEL = '零权限（沙箱默认）';

export default function MarketplaceView({ pluginName, granted }: MarketplaceViewProps) {
  const badges = badgesForGranted(granted);
  return (
    <main style={{ flex: 1, padding: 24, overflow: 'auto' }}>
      <h1 style={{ margin: '0 0 8px', fontSize: 22 }}>插件详情 · {pluginName}</h1>
      <section aria-label="能力徽章">
        <h2 style={{ margin: '16px 0 8px', fontSize: 15, fontWeight: 600 }}>能力面</h2>
        {badges.length === 0 ? (
          <span
            data-testid="badge-zero"
            style={{
              display: 'inline-block',
              padding: '2px 10px',
              borderRadius: 999,
              fontSize: 13,
              border: '1px solid #2f9e44',
              color: '#2f9e44',
              background: 'rgba(47,158,68,0.08)',
            }}
          >
            {ZERO_GRANT_LABEL}
          </span>
        ) : (
          <ul style={{ display: 'flex', gap: 8, flexWrap: 'wrap', listStyle: 'none', margin: 0, padding: 0 }}>
            {badges.map((b) => (
              <li
                key={b.key}
                data-testid={`badge-${b.key}`}
                aria-label={`能力徽章 ${b.label}${b.danger ? '（危险能力）' : ''}`}
                style={{
                  display: 'inline-block',
                  padding: '2px 10px',
                  borderRadius: 999,
                  fontSize: 13,
                  border: b.danger ? '1px solid #e03131' : '1px solid #4c6ef5',
                  color: b.danger ? '#e03131' : '#4c6ef5',
                  background: b.danger ? 'rgba(224,49,49,0.08)' : 'rgba(76,110,245,0.08)',
                }}
              >
                {b.label}
                {b.danger ? ' ⚠' : ''}
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}
