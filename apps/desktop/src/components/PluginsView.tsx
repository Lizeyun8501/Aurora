/**
 * 桌面安装面 UI（DK-市场卡）：已安装插件管理视图。
 *
 * 范围（诚实边界）：
 * - 列表（cmd_plugin_list）/ 卸载（cmd_plugin_uninstall）/ invoke 冒烟（cmd_plugin_invoke）
 * - 安装动作 = 命令层语义（cmd_plugin_install：listing+manifest+本地 wasm 路径），
 *   UI 安装表单随市场下载器卡（下载源未接线前表单无真实输入来源，不做假入口）
 * - 详情徽章复用 MarketplaceView（15c），本组件不重复实现
 */
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

interface PluginsViewProps {
  invoke: InvokeFn;
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

export default function PluginsView({ invoke }: PluginsViewProps) {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [status, setStatus] = useState<string>('');

  const refresh = useCallback(async () => {
    try {
      const rows = (await invoke('cmd_plugin_list')) as InstalledPlugin[];
      setPlugins(rows);
      setStatus(`已安装 ${rows.length} 件`);
    } catch (e) {
      setStatus(`列表失败: ${String(e)}`);
    }
  }, [invoke]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const uninstall = async (id: string) => {
    try {
      await invoke('cmd_plugin_uninstall', { pluginId: id });
      setStatus(`已卸载 ${id}`);
      await refresh();
    } catch (e) {
      setStatus(`卸载失败: ${String(e)}`);
    }
  };

  const smokeInvoke = async (id: string) => {
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

  return (
    <main style={{ flex: 1, padding: 24, overflow: 'auto' }}>
      <h1 style={{ margin: '0 0 4px', fontSize: 22 }}>已安装插件</h1>
      <p data-testid="plugins-status" style={{ margin: '0 0 16px', fontSize: 12, color: '#6b7280' }}>
        {status}
      </p>
      {plugins.length === 0 ? (
        <p style={{ fontSize: 13, color: '#9ca3af' }}>
          暂无已安装插件。安装走命令层语义（cmd_plugin_install：listing + manifest + 本地 wasm 路径），
          UI 安装表单随市场下载器卡。
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
              <button style={S.danger} onClick={() => void uninstall(p.id)}>
                卸载
              </button>
            </div>
          ))}
        </section>
      )}
    </main>
  );
}
