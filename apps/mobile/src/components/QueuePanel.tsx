/**
 * QueuePanel — DK-50 组件 2：离线队列可视化（移植自桌面 DK-48）
 *
 * 逻辑层照搬（queueList 明细 / queueDrain / DrainReport failed_docs
 * 逐条不静默 / onChanged 联动 badge）；形态=全屏底部 sheet（L1482
 * overlay 惯例），样式 mobile.css sync-* 前缀。
 *
 * 停手边界：单条 drain/重试不在本卡（platform 面仅全量 drain）。
 */
import { useCallback, useEffect, useState } from 'react';
import {
  platform,
  type SyncDrainReport,
  type SyncQueueItem,
} from '../adapters/androidPlatform';

interface Props {
  onClose: () => void;
  /** drain 成功后通知壳刷新（badge 状态联动） */
  onChanged?: () => void;
}

export default function QueuePanel({ onClose, onChanged }: Props) {
  const [items, setItems] = useState<SyncQueueItem[]>([]);
  const [report, setReport] = useState<SyncDrainReport | null>(null);
  const [drainErr, setDrainErr] = useState<string | null>(null);
  const [draining, setDraining] = useState(false);

  const refresh = useCallback(() => {
    try {
      setItems(platform.queueList());
    } catch {
      setItems([]);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const drain = useCallback(() => {
    setDraining(true);
    setDrainErr(null);
    try {
      const r = platform.queueDrain();
      if ('error' in r) {
        // 桥/前置错误（未挂接引擎/未配置对端）——不静默，行内展示
        setDrainErr(r.error);
      } else {
        setReport(r);
        onChanged?.();
        refresh();
      }
    } catch (e) {
      setDrainErr(String(e));
    } finally {
      setDraining(false);
    }
  }, [onChanged, refresh]);

  return (
    <>
      <div className="sync-sheet-mask" onClick={onClose} />
      <div className="sync-sheet" role="dialog" aria-label="离线队列">
        <div className="sync-sheet-grip" />
        <div className="sync-sheet-title">
          离线队列{items.length > 0 ? `（积压 ${items.length}）` : ''}
          <button type="button" className="sync-sheet-close" onClick={onClose}>
            关闭
          </button>
        </div>

        {items.length === 0 ? (
          <div className="sync-empty">队列空——全部已同步</div>
        ) : (
          items.map((it) => (
            <div key={it.id} className="sync-item">
              <span className="sync-item-kind">P{it.priority}</span>
              <span className="sync-mono">{it.doc_id.length > 10 ? `${it.doc_id.slice(0, 10)}…` : it.doc_id}</span>
              <div className="sync-item-sub">
                尝试 {it.attempts} 次 · {it.created_at}
              </div>
            </div>
          ))
        )}

        {items.length > 0 && (
          <div className="sync-btn-row">
            <button type="button" className="sync-btn" onClick={drain} disabled={draining}>
              {draining ? '补发中…' : '立即补发（全量）'}
            </button>
          </div>
        )}

        {drainErr && (
          <div className="sync-item-err" role="alert">
            补发未执行：{drainErr}
          </div>
        )}

        {report && (
          <div className="sync-item">
            <div>
              补发完成：成功 {report.synced_docs} 文档 · ack {report.acked_items} 项
            </div>
            {report.failed_docs.length > 0 ? (
              <>
                <div className="sync-fail">失败 {report.failed_docs.length} 条（留队待重试，不静默）：</div>
                {report.failed_docs.map((d) => (
                  <div key={d} className="sync-mono sync-fail">
                    {d}
                  </div>
                ))}
              </>
            ) : (
              <div className="sync-item-sub" style={{ color: 'var(--success)' }}>
                无失败项
              </div>
            )}
          </div>
        )}
      </div>
    </>
  );
}
