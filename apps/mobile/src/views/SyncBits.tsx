/**
 * SyncBits — B-02 切片二: SyncPanel/WifiOnlyRow 从 MobileApp 拆出
 * （SettingsView 依赖;纯搬移零逻辑改动）。
 */
import React, { useEffect, useState } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, Switch } from '../components/primitives';

export function WifiOnlyRow({ showToast }: { showToast: (t: string) => void }) {
  const [on, setOn] = useState(false);
  useEffect(() => {
    setOn(platform.getWifiOnly());
  }, []);
  return (
    <div className="settings-row">
      <span className="settings-row-icon">{I.wifi}</span>
      <span className="settings-row-label">
        仅 Wi-Fi 同步
        <div className="sub">蜂窝网络下暂停 P2P 同步，回到 Wi-Fi 自动恢复</div>
      </span>
      <Switch
        checked={on}
        onChange={(v) => {
          setOn(v);
          platform.setWifiOnly(v);
          showToast(v ? '仅 Wi-Fi 同步已开启' : '仅 Wi-Fi 同步已关闭');
        }}
      />
    </div>
  );
}

export function SyncPanel({ noteId }: { noteId: string }) {
  const [localAddr, setLocalAddr] = useState<string | null>(null);
  const [peerAddr, setPeerAddr] = useState('');
  const [status, setStatus] = useState('');

  const startEngine = () => {
    const addr = platform.startSyncEngine();
    setLocalAddr(addr);
    setStatus(addr ? '引擎已启动，接收循环已开启' : '引擎启动失败（可能无网络权限）');
  };

  const doSync = () => {
    if (!peerAddr.trim()) { setStatus('请输入对端地址'); return; }
    setStatus('同步中…');
    const report = platform.syncNote(peerAddr.trim(), noteId);
    if (!report) { setStatus('同步不可用（仅真机 Android）'); return; }
    setStatus(report.success ? `同步成功：↑${report.sentBytes}B ↓${report.receivedBytes}B` : `同步失败: ${report.error || '未知错误'}`);
  };

  return (
    <div className="sync-panel">
      <div className="sync-row">
        <button className="btn btn-secondary" onClick={startEngine}>启动引擎</button>
        {localAddr && <span className="sync-addr" title={localAddr}>{localAddr.slice(0, 48)}…</span>}
      </div>
      <div className="sync-row">
        <input
          className="input"
          style={{ flex: 1 }}
          value={peerAddr}
          onChange={(e) => setPeerAddr(e.target.value)}
          placeholder='粘贴对端地址 JSON {"ip":…}'
        />
        <button className="btn btn-primary" onClick={doSync}>同步</button>
      </div>
      {status && <div className="sync-status">{status}</div>}
    </div>
  );
}
