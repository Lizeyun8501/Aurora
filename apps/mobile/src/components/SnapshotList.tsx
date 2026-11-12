/**
 * SnapshotList — B-02 切片三: 从 MobileApp 拆出（纯搬移零逻辑改动）。
 */
import React, { useEffect, useState } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, relativeTime } from './primitives';

/** 时间机器快照列表 — JNI listSnapshots/getSnapshotContent 桥（V23-I5 移动端）。 */
export function SnapshotList({ noteId, refreshKey }: { noteId: string; refreshKey: number }) {
  const [metas, setMetas] = useState<Array<{ version: number; created_at: string; size: number }>>([]);
  const [viewing, setViewing] = useState<{ version: number; content: string } | null>(null);

  useEffect(() => {
    try {
      const anyPlatform = platform as unknown as Record<string, unknown>;
      if (typeof anyPlatform.listSnapshots !== 'function') return; // 非 Android 环境守卫
      const arr = JSON.parse(platform.listSnapshots(noteId) || '[]');
      setMetas(Array.isArray(arr) ? arr : []);
    } catch { setMetas([]); }
  }, [noteId, refreshKey]);

  if (!metas.length) {
    return <div style={{ fontSize: 12, opacity: 0.6, marginTop: 6 }}>暂无快照（保存后自动生成）</div>;
  }
  return (
    <div style={{ marginTop: 8 }}>
      {metas.map((m) => (
        <div
          key={m.version}
          className="settings-row"
          style={{ padding: '8px 0', borderTop: '1px solid var(--border)' }}
          onClick={() => {
            try {
              const c = platform.getSnapshotContent(noteId, m.version);
              setViewing({ version: m.version, content: c ?? '（读取失败）' });
            } catch { setViewing({ version: m.version, content: '（读取失败）' }); }
          }}
        >
          <span className="settings-row-label">
            版本 {m.version}
            <div className="sub">{m.created_at} · {m.size} 字节 · 点按查看存档正文</div>
          </span>
          <span className="settings-value">查看</span>
        </div>
      ))}
      {viewing && (
        <>
          <div className="sheet-mask" onClick={() => setViewing(null)} />
          <div className="sheet">
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">版本 {viewing.version} 存档</span>
              <button className="icon-btn" onClick={() => setViewing(null)} aria-label="关闭">{I.x}</button>
            </div>
            <div className="sheet-body">
              <pre style={{ whiteSpace: 'pre-wrap', fontSize: 14, lineHeight: 1.6, fontFamily: 'inherit', margin: 0 }}>
                {viewing.content}
              </pre>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
