// Aurora Desktop 前端引导 — V20 P0-2 骨架
// 加载顺序: Tauri IPC 探测 → PlatformAPI 注入 → （下阶段）React Shell 挂载
import { tryLoadTauriApis } from './adapters/ipc';
import { tauriPlatform } from './adapters/tauriPlatform';

async function boot() {
  const apis = tryLoadTauriApis();
  const mode = apis ? 'tauri' : 'browser-mock';
  const platform = tauriPlatform;
  const info = {
    mode,
    platform: platform.constructor.name,
    build: import.meta.env.MODE,
  };
  const root = document.getElementById('root');
  if (root) {
    root.innerHTML = `
      <main style="font-family: system-ui; padding: 32px; color: #1F2937;">
        <h1 style="margin:0 0 8px; font-size:24px;">Aurora Note Desktop</h1>
        <p style="margin:0; color:#6B7280; font-size:15px;">
          IPC: ${info.mode} · Platform: ${info.platform} · Build: ${info.build}
        </p>
        <p style="margin:16px 0 0; color:#9CA3AF; font-size:13px;">
          界面 Shell 按 V20 §5 于下阶段实施（本骨架保障 dev/build 闭环与 CoreAPI 注入）。
        </p>
      </main>`;
  }
  console.info('[aurora-desktop] booted', info);
}

boot();
