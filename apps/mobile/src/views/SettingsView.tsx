/**
 * SettingsView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import {
  THEME_WHITELIST, THEME_DEFAULTS, THEME_PRESETS,
  validateTheme, applyTheme, clearTheme, exportThemeJSON, importThemeJSON,
  type ThemeTokenKey, type ThemeOverrides,
} from '@aurora/ui-components'; // A-11: 主题 VM（白名单+对比度——双端同源）
import { I, NoteCard, EmptyState, relativeTime, type Task } from '../components/primitives';
import { SyncPanel, WifiOnlyRow } from './SyncBits';

export function SettingsView({ theme, onTheme, showToast }: {
  theme: 'light' | 'dark' | 'system';
  onTheme: (t: 'light' | 'dark' | 'system') => void;
  showToast: (t: string) => void;
}) {
  const [syncOpen, setSyncOpen] = useState(false);
  const soon = (name: string) => showToast(`${name}即将上线`);
  /* ── A-11: 自定义主题（白名单覆盖——双端同源 VM）────────────── */
  const [customOv, setCustomOv] = useState<ThemeOverrides>(() => {
    try {
      return JSON.parse(localStorage.getItem('aurora.theme.custom') ?? '{}') as ThemeOverrides;
    } catch { return {}; }
  });
  const [themeErrs, setThemeErrs] = useState<string[]>([]);
  const [importText, setImportText] = useState('');
  const [importOpen, setImportOpen] = useState(false);
  const liveOv = { ...THEME_DEFAULTS, ...customOv }; // 实时预览（未应用=仅预览态）
  const tryApply = (ov: ThemeOverrides): boolean => {
    const v = validateTheme(ov);
    setThemeErrs(v.errors);
    if (!v.ok) return false;
    applyTheme(ov);
    try { localStorage.setItem('aurora.theme.custom', JSON.stringify(ov)); } catch { /* 隐私模式 */ }
    return true;
  };

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

      {/* A-11: 自定义主题——白名单覆盖+对比度硬约束+实时预览+导入导出 */}
      <div className="settings-group">
        <div className="settings-group-title">主题定制（A-11）</div>
        <div className="settings-card">
          <div className="settings-row" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span className="settings-row-icon">🎨</span>
              <span className="settings-row-label">预设方案（{THEME_PRESETS.length} 套——点选即应用）</span>
            </div>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {THEME_PRESETS.map((p) => (
                <button
                  key={p.name}
                  className={`task-prio-seg ${Object.keys(customOv).length > 0 && JSON.stringify(customOv) === JSON.stringify(p.overrides) ? 'on' : ''}`}
                  onClick={() => { if (tryApply(p.overrides)) showToast(`已应用预设「${p.name}」`); }}
                  aria-label={`应用预设 ${p.name}`}
                  style={{ display: 'flex', alignItems: 'center', gap: 6 }}
                >
                  <span style={{
                    width: 16, height: 16, borderRadius: 4, display: 'inline-block',
                    border: '1px solid rgba(255,255,255,0.2)',
                    background: `linear-gradient(135deg, ${(p.overrides.bgBase ?? THEME_DEFAULTS.bgBase)} 50%, ${(p.overrides.primary ?? THEME_DEFAULTS.primary)} 50%)`,
                  }} />
                  {p.name}
                </button>
              ))}
            </div>
          </div>
          <div className="settings-row" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 8 }}>
            <div className="settings-row-label">自定义颜色（白名单 {THEME_WHITELIST.length} 项——其余令牌锁定）</div>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(150px, 1fr))', gap: 6 }}>
              {THEME_WHITELIST.map((k: ThemeTokenKey) => (
                <label key={k} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
                  <input
                    type="color"
                    aria-label={`自定义 ${k}`}
                    value={liveOv[k]}
                    onChange={(e) => setCustomOv((ov) => ({ ...ov, [k]: e.target.value }))}
                    onBlur={() => { if (tryApply(customOv)) showToast('主题已应用（对比度校验通过）'); }}
                  />
                  <span style={{ opacity: 0.85 }}>{k}</span>
                </label>
              ))}
            </div>
            {themeErrs.length > 0 && (
              <div role="alert" style={{ color: 'var(--danger, #E85D5D)', fontSize: 12, whiteSpace: 'pre-wrap' }}>
                {themeErrs.join('\n')}
              </div>
            )}
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              <button className="btn btn-secondary" onClick={() => { if (tryApply(customOv)) showToast('主题已应用'); }}>应用</button>
              <button className="btn btn-secondary" onClick={() => {
                clearTheme();
                setCustomOv({});
                setThemeErrs([]);
                try { localStorage.removeItem('aurora.theme.custom'); } catch { /* 防御 */ }
                showToast('已恢复默认主题');
              }}>恢复默认</button>
              <button
                className="btn btn-secondary"
                onClick={() => {
                  const json = exportThemeJSON(customOv);
                  if (navigator.share) {
                    navigator.share({ title: 'Aurora 主题', text: json }).catch(() => showToast('分享取消'));
                  } else if (navigator.clipboard) {
                    navigator.clipboard.writeText(json).then(() => showToast('主题 JSON 已复制'), () => showToast('复制失败'));
                  } else {
                    showToast('导出通道不可用');
                  }
                }}
              >导出 JSON</button>
              <button className="btn btn-secondary" onClick={() => setImportOpen((o) => !o)} aria-expanded={importOpen}>导入 JSON</button>
            </div>
            {importOpen && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                <textarea
                  className="input"
                  aria-label="主题 JSON 粘贴区"
                  placeholder="粘贴主题 JSON（type=aurora-theme）"
                  value={importText}
                  onChange={(e) => setImportText(e.target.value)}
                  style={{ minHeight: 72, fontSize: 12 }}
                />
                <div>
                  <button className="btn btn-primary" onClick={() => {
                    const r = importThemeJSON(importText);
                    if (r.overrides) {
                      if (tryApply(r.overrides)) { setCustomOv(r.overrides); setImportOpen(false); setImportText(''); showToast('主题已导入并应用'); }
                    } else {
                      setThemeErrs(r.errors);
                    }
                  }}>导入并应用</button>
                </div>
              </div>
            )}
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
