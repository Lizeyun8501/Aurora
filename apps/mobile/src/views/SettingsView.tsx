/**
 * SettingsView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, type Task } from '../components/primitives';
import { SyncPanel, WifiOnlyRow } from './SyncBits';

export function SettingsView({ theme, onTheme, showToast }: {
  theme: 'light' | 'dark' | 'system';
  onTheme: (t: 'light' | 'dark' | 'system') => void;
  showToast: (t: string) => void;
}) {
  const [syncOpen, setSyncOpen] = useState(false);
  const soon = (name: string) => showToast(`${name}即将上线`);

  return (
    <div className="view">
      <div className="settings-group">
        <div className="settings-group-title">外观</div>
        <div className="settings-card">
          <div className="settings-row" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span className="settings-row-icon">{theme === 'dark' ? I.moon : I.sun}</span>
              <span className="settings-row-label">外观模式</span>
            </div>
            {/* E10b: 三段选择——浅色/深色/跟随系统（TA 14:03） */}
            <div style={{ display: 'flex', gap: 6 }}>
              {([['light', '☀️ 浅色'], ['dark', '🌙 深色'], ['system', '⚙️ 跟随系统']] as const).map(([val, label]) => (
                <button key={val} className={`nt-seg ${theme === val ? 'on' : ''}`}
                  style={{ flex: 1, height: 34, borderRadius: 8, border: 'none', cursor: 'pointer', fontSize: 12 }}
                  onClick={() => onTheme(val)}>{label}</button>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">Mirror 导出 — 活的笔记库</div>
        <div className="settings-card">
          <div className="settings-row">
            <span className="settings-row-icon">{I.note}</span>
            <span className="settings-row-label">
              Markdown 单向导出
              <div className="sub">每次保存自动写入 data/mirror/（YAML 前置）</div>
            </span>
            <span className="settings-value">自动</span>
          </div>
          <div className="settings-row">
            <span className="settings-row-icon">{I.text}</span>
            <span className="settings-row-label">
              用 Obsidian 打开该目录
              <div className="sub">即得与笔记应用同源的 Markdown 库 · 单向只读</div>
            </span>
            <span className="settings-value">v20</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">同步</div>
        <div className="settings-card">
          <div className="settings-row" onClick={() => setSyncOpen(!syncOpen)}>
            <span className="settings-row-icon">{I.sync}</span>
            <span className="settings-row-label">P2P 同步<div className="sub">iroh · NAT 穿透</div></span>
            <span className="settings-value">{syncOpen ? '收起' : '展开'}</span>
          </div>
          {syncOpen && (
            <div style={{ padding: '4px 12px 12px' }}><SyncPanel noteId="" /></div>
          )}
          <WifiOnlyRow showToast={showToast} />
          <div className="settings-row" onClick={() => soon('冲突管理')}>
            <span className="settings-row-icon">{I.note}</span>
            <span className="settings-row-label">冲突管理</span>
            <span className="settings-value">0 个冲突</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">智能</div>
        <div className="settings-card">
          <div className="settings-row" onClick={() => soon('AI 模型配置')}>
            <span className="settings-row-icon">{I.spark}</span>
            <span className="settings-row-label">AI 模型</span>
            <span className="settings-value">未配置</span>
          </div>
          <div className="settings-row" onClick={() => soon('OCR')}>
            <span className="settings-row-icon">{I.camera}</span>
            <span className="settings-row-label">OCR 能力</span>
            <span className="settings-value">未启用</span>
          </div>
          <div className="settings-row" onClick={() => soon('知识图谱')}>
            <span className="settings-row-icon">{I.today}</span>
            <span className="settings-row-label">知识图谱</span>
            <span className="settings-value">›</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">安全</div>
        <div className="settings-card">
          <div className="settings-row">
            <span className="settings-row-icon">{I.lock}</span>
            <span className="settings-row-label">工作空间<div className="sub">🔒 私有 · 端到端加密</div></span>
            <span className="settings-value">ML-KEM-768</span>
          </div>
          <div className="settings-row" onClick={() => soon('审计日志')}>
            <span className="settings-row-icon">{I.note}</span>
            <span className="settings-row-label">安全审计日志</span>
            <span className="settings-value">›</span>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-group-title">关于</div>
        <div className="settings-card">
          <div className="settings-row"><span className="settings-row-label">版本</span><span className="settings-value">0.13.0 · V19 UI</span></div>
          <div className="settings-row"><span className="settings-row-label">架构</span><span className="settings-value">Rust Core + React</span></div>
          <div className="settings-row"><span className="settings-row-label">引擎</span><span className="settings-value">Loro CRDT · SQLite · iroh</span></div>
        </div>
      </div>
    </div>
  );
}
