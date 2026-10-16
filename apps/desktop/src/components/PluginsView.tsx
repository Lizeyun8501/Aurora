/**
 * 桌面安装面 UI（DK-市场卡）：已安装插件管理视图。
 *
 * 范围（诚实边界）：
 * - 列表（cmd_plugin_list）/ 卸载（cmd_plugin_uninstall）/ invoke 冒烟（cmd_plugin_invoke）
 * - 安装动作 = 命令层语义（cmd_plugin_install：listing+manifest+本地 wasm 路径），
 *   UI 安装表单随市场下载器卡（下载源未接线前表单无真实输入来源，不做假入口）
 * - 详情徽章复用 MarketplaceView（15c），本组件不重复实现
 */
import type React from 'react';
import { useCallback, useEffect, useState } from 'react';

/** invoke 形状（与 TrashView/SyncStatusBadge 本地惯例一致）。 */
type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

/** 已安装插件行（PluginManifest 桌面投影——仅 UI 关心的字段子集）。 */
interface InstalledPlugin {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  permissions: readonly string[];
}

/** 市场 listing 行（Rust PluginListing 投影——仅 UI 关心的字段子集）。 */
interface MarketListing {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  source: string;
  verified: boolean;
}

interface PluginsViewProps {
  invoke: InvokeFn | null;
}

/** 审计行（AuditEntry tauri 投影——字段与 Rust serde Serialize 对齐）。 */
interface AuditEntry {
  ts: string;
  plugin: string;
  event: string;
  result: string;
  reason: string;
  detail: string;
}

const S = {
  row: {
    display: 'flex',
    alignItems: 'center',
    gap: 12,
    padding: '10px 12px',
    borderBottom: '1px solid #e5e7eb',
  } as const,
  name: { fontSize: 14, fontWeight: 600 } as const,
  meta: { fontSize: 12, color: '#6b7280' } as const,
  danger: {
    padding: '4px 10px',
    fontSize: 12,
    border: '1px solid #fecaca',
    borderRadius: 6,
    background: '#fef2f2',
    color: '#b91c1c',
    cursor: 'pointer',
  } as const,
  smoke: {
    padding: '4px 10px',
    fontSize: 12,
    border: '1px solid #e5e7eb',
    borderRadius: 6,
    background: '#fff',
    cursor: 'pointer',
  } as const,
};

export default function PluginsView({ invoke }: PluginsViewProps): React.ReactElement {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [listings, setListings] = useState<MarketListing[]>([]);
  const [installing, setInstalling] = useState<string>('');
  const [status, setStatus] = useState<string>('');
  /** 审计链查看态（DK-审计面：选中插件的最近审计行） */
  const [audit, setAudit] = useState<AuditEntry[]>([]);

  const refresh = useCallback(async () => {
    if (!invoke) return;
    try {
      const rows = (await invoke('cmd_plugin_list')) as InstalledPlugin[];
      setPlugins(rows);
      setStatus(`已安装 ${rows.length} 件`);
    } catch (e) {
      setStatus(`列表失败: ${String(e)}`);
    }
  }, [invoke]);

  const refreshMarket = useCallback(async () => {
    if (!invoke) return;
    try {
      const rows = (await invoke('cmd_market_list')) as MarketListing[];
      setListings(rows);
    } catch (e) {
      setStatus(`市场列表失败: ${String(e)}`);
    }
  }, [invoke]);

  useEffect(() => {
    void refresh();
    void refreshMarket();
  }, [refresh, refreshMarket]);

  const uninstall = async (id: string) => {
    if (!invoke) return;
    try {
      await invoke('cmd_plugin_uninstall', { pluginId: id });
      setStatus(`已卸载 ${id}`);
      await refresh();
    } catch (e) {
      setStatus(`卸载失败: ${String(e)}`);
    }
  };

  const smokeInvoke = async (id: string) => {
    if (!invoke) return;
    try {
      const out = await invoke('cmd_plugin_invoke', {
        pluginId: id,
        method: 'handle-event',
        args: { event: 'install-smoke' },
      });
      setStatus(`${id} invoke → ${JSON.stringify(out)}`);
    } catch (e) {
      setStatus(`${id} invoke 拒绝/失败: ${String(e)}`);
    }
  };
  /** 审计链查看（DK-审计面：内存环形缓冲最近 50 条，按插件过滤）。 */
  const showAudit = async (id: string) => {
    if (!invoke) return;
    try {
      const rows = (await invoke('cmd_plugin_audit_logs', {
        limit: 50,
        pluginId: id,
      })) as AuditEntry[];
      setAudit(rows);
      setStatus(`审计链 ${id}：${rows.length} 条`);
    } catch (e) {
      setStatus(`审计查询失败: ${String(e)}`);
    }
  };

  /** 市场安装（语义层 to_manifest 派生——UI 不拼 manifest，DK-43 传播收口）。 */
  const installFromMarket = async (l: MarketListing) => {
    if (!invoke) return;
    setInstalling(l.id);
    setStatus(`正在安装 ${l.id}（${l.source}${l.verified ? ' · verified' : ' · 未 verified'}）…`);
    try {
      const m = (await invoke('cmd_market_install', {
        listingId: l.id,
        manifest: null,
        requireVerified: true,
      })) as InstalledPlugin;
      setStatus(`已安装 ${m.id} v${m.version}`);
      await refresh();
    } catch (e) {
      // 拒装原因透传（未 verified/下载失败/checksum 不匹配等——installer 错误串）
      setStatus(`安装失败: ${String(e)}`);
    } finally {
      setInstalling('');
    }
  };

  return (
    <main style={{ flex: 1, padding: 24, overflow: 'auto' }}>
      <h1 style={{ margin: '0 0 4px', fontSize: 22 }}>已安装插件</h1>
      <p data-testid="plugins-status" style={{ margin: '0 0 16px', fontSize: 12, color: '#6b7280' }}>
        {status}
      </p>
      {plugins.length === 0 ? (
        <p style={{ fontSize: 13, color: '#9ca3af' }}>
          暂无已安装插件。可从下方市场分区安装（listing → 下载 → 签名验证 → 六环安装）。
        </p>
      ) : (
        <section aria-label="已安装插件列表">
          {plugins.map((p) => (
            <div key={p.id} style={S.row}>
              <div style={{ flex: 1 }}>
                <div style={S.name}>
                  {p.name} <span style={S.meta}>v{p.version}</span>
                </div>
                <div style={S.meta}>
                  {p.author} · 权限 {p.permissions.length > 0 ? p.permissions.join(', ') : '零权限（沙箱默认）'}
                </div>
              </div>
              <button style={S.smoke} onClick={() => void smokeInvoke(p.id)}>
                invoke 冒烟
              </button>
              <button style={S.smoke} onClick={() => void showAudit(p.id)}>
                审计
              </button>
              <button style={S.danger} onClick={() => void uninstall(p.id)}>
                卸载
              </button>
            </div>
          ))}
        </section>
      )}

      <h2 style={{ margin: '24px 0 8px', fontSize: 16 }}>市场可用件</h2>
      {listings.length === 0 ? (
        <p style={{ fontSize: 13, color: '#9ca3af' }}>
          市场暂无 listing（发布/注册走命令层或后续市场后端卡）。
        </p>
      ) : (
        <section aria-label="市场可用件列表">
          {listings.map((l) => (
            <div key={l.id} style={S.row}>
              <div style={{ flex: 1 }}>
                <div style={S.name}>
                  {l.name} <span style={S.meta}>v{l.version} · {l.source}</span>
                </div>
                <div style={S.meta}>
                  {l.verified ? '✓ 已验证' : '未验证'}
                  {' · '}
                  {l.description}
                </div>
              </div>
              <button
                style={S.smoke}
                disabled={installing === l.id}
                data-testid={`market-install-${l.id}`}
                onClick={() => void installFromMarket(l)}
              >
                {installing === l.id ? '安装中…' : '安装'}
              </button>
            </div>
          ))}
        </section>
      )}
      {audit.length > 0 && (
        <section aria-label="审计链" style={{ marginTop: 16 }}>
          <h2 style={{ margin: '0 0 8px', fontSize: 14, fontWeight: 600 }}>审计链（最近 {audit.length} 条）</h2>
          {audit.map((a, i) => (
            <div key={i} style={{ padding: '4px 0', borderBottom: '1px solid #f3f4f6', fontSize: 12 }}>
              <span style={{ color: '#9ca3af' }}>{a.ts}</span>{' '}
              <span style={{ color: a.result === 'ok' ? '#16a34a' : '#dc2626' }}>
                {a.event}/{a.result}
              </span>{' '}
              {a.reason !== '-' && <span style={S.meta}>{a.reason}</span>}{' '}
              {a.detail && <span style={S.meta}>{a.detail}</span>}
            </div>
          ))}
        </section>
      )}
    </main>
  );
}
