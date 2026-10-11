/**
 * QueuePanel — DK-48 组件 2：离线队列可视化
 *
 * 数据源（只读消费，Rust 零变更）：
 * - cmd_queue_list → QueueItemDto 明细表格（字段全展示）
 * - cmd_queue_drain → DrainReportDto 结果报告（成功/ack/失败条目列出，
 *   失败不静默 — DoD2）
 *
 * 交互跟随壳内浮层惯例（fixed 遮罩 + 居中 dialog + Esc 关闭）；
 * drain 后回调 onChanged（壳侧刷新 badge 状态）。
 * 停手边界：单条 drain/重试不在本卡（cmd_queue_drain 仅全量）。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

/** 与 sync_commands.rs QueueItemDto 对齐。 */
interface QueueItemDto {
  id: string;
  doc_id: string;
  priority: number;
  attempts: number;
  created_at: string;
}

/** 与 sync_commands.rs DrainReportDto 对齐。 */
interface DrainReportDto {
  synced_docs: number;
  failed_docs: string[];
  acked_items: number;
}

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface Props {
  invoke: InvokeFn;
  onClose: () => void;
  /** drain 成功后通知壳刷新（badge 状态/面板条目计数） */
  onChanged?: () => void;
}

export default function QueuePanel({ invoke, onClose, onChanged }: Props) {
  const [items, setItems] = useState<QueueItemDto[] | null>(null);
  const [report, setReport] = useState<DrainReportDto | null>(null);
  const [drainErr, setDrainErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loadErr, setLoadErr] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke('cmd_queue_list')
      .then((r) => {
        setItems(r as QueueItemDto[]);
        setLoadErr(null);
      })
      .catch((e) => {
        setItems(null);
        setLoadErr(String(e));
      });
  }, [invoke]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  const drain = useCallback(() => {
    setBusy(true);
    setReport(null);
    setDrainErr(null);
    invoke('cmd_queue_drain')
      .then((r) => {
        setReport(r as DrainReportDto);
        setBusy(false);
        refresh();
        onChanged?.();
      })
      .catch((e) => {
        setDrainErr(String(e));
        setBusy(false);
      });
  }, [invoke, refresh, onChanged]);

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(8, 12, 18, 0.72)',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        zIndex: 1000,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="离线队列"
        style={{
          width: 680,
          maxWidth: '94vw',
          maxHeight: '80vh',
          display: 'flex',
          flexDirection: 'column',
          background: tokens.color.bgSurface,
          borderRadius: tokens.radius.lg,
          border: '1px solid rgba(55, 220, 242, 0.25)',
          boxShadow: '0 24px 64px rgba(0,0,0,0.5)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: tokens.spacing.md,
            borderBottom: '1px solid rgba(255,255,255,0.08)',
          }}
        >
          <strong style={{ fontSize: tokens.typography.heading.size }}>
            离线队列{items ? `（积压 ${items.length} 条）` : ''}
          </strong>
          <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
            <button
              type="button"
              onClick={drain}
              disabled={busy || !items || items.length === 0}
              title={
                !items || items.length === 0
                  ? '队列空，无需补发'
                  : '全量补发到同步对端（需已配对且在线）'
              }
              style={{
                ...actionBtnStyle,
                opacity: busy || !items || items.length === 0 ? 0.5 : 1,
                cursor: busy || !items || items.length === 0 ? 'not-allowed' : 'pointer',
              }}
            >
              {busy ? '补发中…' : '全量补发'}
            </button>
            <button type="button" onClick={onClose} style={actionBtnStyle} aria-label="关闭">
              关闭
            </button>
          </div>
        </div>
        <div style={{ padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`, overflowY: 'auto' }}>
          {loadErr && (
            <div style={{ color: tokens.color.danger, padding: tokens.spacing.md }}>
              队列查询失败：{loadErr}
            </div>
          )}
          {items !== null && items.length === 0 && (
            <div
              style={{
                padding: `${tokens.spacing.xl}px ${tokens.spacing.md}px`,
                textAlign: 'center',
                color: tokens.color.textSecondary,
              }}
            >
              队列空——全部已同步
            </div>
          )}
          {items !== null && items.length > 0 && (
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: tokens.typography.caption.size }}>
              <thead>
                <tr style={{ color: tokens.color.textSecondary, textAlign: 'left' }}>
                  <th style={thTdStyle}>优先级</th>
                  <th style={thTdStyle}>文档</th>
                  <th style={thTdStyle}>尝试</th>
                  <th style={thTdStyle}>入队时间</th>
                </tr>
              </thead>
              <tbody>
                {items.map((it) => (
                  <tr key={it.id} style={{ borderTop: '1px solid rgba(255,255,255,0.06)' }}>
                    <td style={thTdStyle}>
                      <span
                        title="Priority 权重（数值越小越优先补发）"
                        style={{
                          background: tokens.color.bgElevated,
                          borderRadius: tokens.radius.sm,
                          padding: '1px 6px',
                          fontWeight: 600,
                        }}
                      >
                        P{it.priority}
                      </span>
                    </td>
                    <td
                      style={{ ...thTdStyle, fontFamily: tokens.typography.mono }}
                      title={it.doc_id}
                    >
                      {it.doc_id.length > 18 ? `${it.doc_id.slice(0, 18)}…` : it.doc_id}
                    </td>
                    <td style={thTdStyle}>{it.attempts}</td>
                    <td style={{ ...thTdStyle, color: tokens.color.textSecondary }}>
                      {it.created_at}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {drainErr && (
            <div
              role="alert"
              style={{
                marginTop: tokens.spacing.md,
                padding: tokens.spacing.md,
                background: tokens.color.bgElevated,
                border: `1px solid ${tokens.color.danger}`,
                borderRadius: tokens.radius.md,
                color: tokens.color.danger,
              }}
            >
              补发失败：{drainErr}
            </div>
          )}
          {report && (
            <div
              role="status"
              style={{
                marginTop: tokens.spacing.md,
                padding: tokens.spacing.md,
                background: tokens.color.bgElevated,
                border: `1px solid ${report.failed_docs.length > 0 ? tokens.color.warning : tokens.color.success}`,
                borderRadius: tokens.radius.md,
              }}
            >
              <div style={{ fontWeight: 600, marginBottom: tokens.spacing.xs }}>
                补发报告：成功 {report.synced_docs} 文档 · ack {report.acked_items} 项
              </div>
              {report.failed_docs.length > 0 ? (
                <>
                  <div style={{ color: tokens.color.warning, marginBottom: tokens.spacing.xs }}>
                    失败 {report.failed_docs.length} 条（留队待重试，不静默）：
                  </div>
                  <div style={{ fontFamily: tokens.typography.mono, fontSize: 12 }}>
                    {report.failed_docs.join('、')}
                  </div>
                </>
              ) : (
                <div style={{ color: tokens.color.success }}>无失败项</div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

const thTdStyle: React.CSSProperties = {
  padding: `${tokens.spacing.sm}px ${tokens.spacing.sm}px`,
  textAlign: 'left',
};

const actionBtnStyle: React.CSSProperties = {
  background: tokens.color.bgElevated,
  border: '1px solid rgba(255,255,255,0.10)',
  borderRadius: tokens.radius.md,
  padding: `${tokens.spacing.sm + 2}px ${tokens.spacing.md}px`,
  color: tokens.color.textPrimary,
  fontSize: tokens.typography.caption.size,
};
