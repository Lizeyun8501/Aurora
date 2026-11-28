/**
 * MarketplaceView — F-11: 移动端插件市场列表页（只读面——列/查）。
 *
 * 数据链: platform.marketplaceList()（真桥=Rust Marketplace 注册表;
 * mock=localStorage aurora.plugins 同契约 snake 形——E2E 可造数）。
 * 条目形态: { id,name,version,author,description,source,verified } —
 * Rust PluginListing serde snake_case 镜像。
 *
 * 非目标（停手边界）: 安装/信任切换/签名验证交互——另卡（DK-14 系列已立）。
 */
import React, { useMemo, useState } from 'react';
import { platform } from '../adapters/androidPlatform';
import { EmptyState } from '@aurora/ui-components';
import { I } from '../components/primitives';

/** Rust PluginListing TS 镜像（只读消费面——snake_case 不转 camel,桥契约即 UI 契约）。 */
interface PluginListingDto {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  source: 'Official' | 'ThirdParty' | 'Local';
  verified: boolean;
}

const SOURCE_LABEL: Record<PluginListingDto['source'], string> = {
  Official: '官方',
  ThirdParty: '第三方',
  Local: '本地',
};

function parseListings(raw: string): PluginListingDto[] {
  try {
    const arr = JSON.parse(raw) as Array<Record<string, unknown>>;
    if (!Array.isArray(arr)) return [];
    return arr.filter((x) =>
      typeof x.id === 'string' && typeof x.name === 'string') as unknown as PluginListingDto[];
  } catch {
    return [];
  }
}

export default function MarketplaceView() {
  const [sel, setSel] = useState<PluginListingDto | null>(null);
  const listings = useMemo(() => parseListings(platform.marketplaceList()), [sel]);

  if (sel) {
    // 详情页（返回列表/信任说明——只读展示,无安装动作）
    return (
      <div className="view">
        <header className="page-head">
          <button className="btn btn-text" onClick={() => setSel(null)} aria-label="返回插件列表">← 返回</button>
          <span className="page-title">{sel.name}</span>
        </header>
        <div className="card" style={{ padding: 16 }}>
          <div className="mk-detail-head">
            <span className="mk-icon">{I.agent}</span>
            <div>
              <div className="mk-name">{sel.name}</div>
              <div className="mk-author">{sel.author}</div>
            </div>
            <span className={`mk-src ${sel.source === 'Official' ? 'official' : ''}`}>
              {SOURCE_LABEL[sel.source]}
            </span>
          </div>
          <div className="mk-version">v{sel.version}{sel.verified ? ' · 已验证' : ' · 未验证'}</div>
          <p className="mk-desc">{sel.description}</p>
          <div className="mk-note" role="note">
            安装与信任管理将在后续版本提供（当前为只读市场）。
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="view">
      <header className="page-head">
        <span className="page-title">插件市场</span>
      </header>
      {listings.length === 0 ? (
        <EmptyState illustration="generic" title="市场暂无插件。" hint="插件发布后将在列表中出现" />
      ) : (
        <div className="mk-list" role="list">
          {listings.map((l) => (
            <button key={l.id} className="mk-item" role="listitem"
              aria-label={`查看插件 ${l.name} v${l.version}`}
              onClick={() => setSel(l)}>
              <span className="mk-icon">{I.agent}</span>
              <div className="mk-item-body">
                <div className="mk-name">{l.name}</div>
                <div className="mk-desc">{l.description}</div>
                <div className="mk-meta">
                  v{l.version} · {SOURCE_LABEL[l.source]}
                  {l.verified && <span className="mk-verified" role="img" aria-label="已验证"> ✓</span>}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
