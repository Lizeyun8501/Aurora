/**
 * SyncStatusBadge — DK-48 组件 1：常驻同步指示（顶栏缝隙挂载）
 *
 * 数据源：cmd_sync_status 轮询（15s + 挂载即刷；不做事件订阅——
 * EventBus 桥接超范围，见工单 §4 停手边界）。
 *
 * 展示（DoD1 三信息主界面直读）：
 * - gate 三态图标 + 文字（颜色不作唯一载体 — tokens 设计原则）
 * - queue_len 徽标（>0 显示）
 * - 冲突红点（pending_artifacts + pending_semantic > 0）
 *
 * 点击浮层：状态明细 + 三快捷入口（队列面板 / 冲突面板 / 仅 Wi-Fi 开关）。
 * 浮层交互跟随壳内既有浮层惯例（fixed 遮罩 + Esc/点外关闭）。
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import tokens from '../design/tokens';

/** 与 sync_commands.rs SyncStatusDto 对齐（serde 默认 snake_case 透传）。 */
interface SyncStatusDto {
  gate: string;
  wifi_only: boolean;
  queue_len: number;
  pending_artifacts: number;
  pending_semantic: number;
}

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface Props {
  invoke: InvokeFn;
  /** 壳内 wifi_only 状态（null = 未加载/不可用 — 浮层内开关置灰） */
  wifiOnly: boolean | null;
  onToggleWifiOnly: () => void;
  onOpenQueue: () => void;
  onOpenConflicts: () => void;
}

const POLL_MS = 15_000;

/** gate 值 → 图标/文案/语义色（三态，DeferUntilUnmetered=仅 Wi-Fi 计量网拦截）。 */
function gateView(gate: string): { icon: string; label: string; color: string } {
  switch (gate) {
    case 'Allow':
      return { icon: '✓', label: '同步畅通', color: tokens.color.success };
    case 'DeferUntilUnmetered':
      return { icon: '⏸', label: '门控推迟（仅 Wi-Fi）', color: tokens.color.warning };
    default:
      return { icon: '⏸', label: '离线（已推迟）', color: tokens.color.textSecondary };
  }
}

export default function SyncStatusBadge({
  invoke,
  wifiOnly,
  onToggleWifiOnly,
  onOpenQueue,
  onOpenConflicts,
}: Props) {
  const [status, setStatus] = useState<SyncStatusDto | null>(null);
  const [popupOpen, setPopupOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(() => {
    invoke('cmd_sync_status')
      .then((r) => setStatus(r as SyncStatusDto))
      .catch(() => setStatus(null));
  }, [invoke]);

  // 挂载即刷 + 15s 轮询（工单：不做事件订阅）
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, POLL_MS);
    return () => clearInterval(t);
  }, [refresh]);

  // 浮层打开时点外/Esc 关闭（壳内浮层惯例）
  useEffect(() => {
    if (!popupOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setPopupOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setPopupOpen(false);
    };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDoc);
      document.removeEventListener('keydown', onKey);
    };
  }, [popupOpen]);

  if (!status) return null; // 状态未就绪（mock 模式/查询失败）不占位

  const gv = gateView(status.gate);
  const conflictPending = status.pending_artifacts + status.pending_semantic;

  return (
    <div ref={rootRef} style={{ position: 'relative' }}>
      <button
        type="button"
        onClick={() => setPopupOpen((v) => !v)}
        aria-label={`同步状态：${gv.label}，队列 ${status.queue_len} 条，冲突 ${conflictPending} 条`}
        title={gv.label}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: tokens.spacing.sm,
          background: tokens.color.bgElevated,
          border: '1px solid rgba(255,255,255,0.10)',
          borderRadius: tokens.radius.md,
          padding: `${tokens.spacing.xs + 2}px ${tokens.spacing.sm + 2}px`,
          cursor: 'pointer',
          color: tokens.color.textPrimary,
          fontSize: tokens.typography.caption.size,
          minHeight: tokens.a11y.minTouchTarget - 12,
        }}
      >
        <span style={{ color: gv.color, fontWeight: 700 }} aria-hidden="true">
          {gv.icon}
        </span>
        <span
          style={{
            color: tokens.color.textSecondary,
            maxWidth: 132,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {gv.label}
        </span>
        {status.queue_len > 0 && (
          <span
            title={`离线队列积压 ${status.queue_len} 条`}
            style={{
              background: tokens.color.primary,
              color: '#FFFFFF',
              borderRadius: 999,
              padding: '0 7px',
              fontSize: 11,
              fontWeight: 600,
              lineHeight: '18px',
            }}
          >
            {status.queue_len}
          </span>
        )}
        {conflictPending > 0 && (
          <span
            title={`待处理冲突 ${conflictPending} 条`}
            aria-label={`待处理冲突 ${conflictPending} 条`}
            style={{
              width: 9,
              height: 9,
              borderRadius: 999,
              background: tokens.color.danger,
              display: 'inline-block',
            }}
          />
        )}
      </button>
      {popupOpen && (
        <div
          role="dialog"
          aria-label="同步状态详情"
          style={{
            position: 'absolute',
            top: 'calc(100% + 8px)',
            right: 0,
            width: 320,
            background: tokens.color.bgSurface,
            border: '1px solid rgba(55, 220, 242, 0.25)',
            borderRadius: tokens.radius.lg,
            boxShadow: '0 16px 48px rgba(0,0,0,0.5)',
            padding: tokens.spacing.md,
            zIndex: 900,
          }}
        >
          <div
            style={{
              fontSize: tokens.typography.heading.size,
              fontWeight: 600,
              marginBottom: tokens.spacing.sm,
            }}
          >
            同步状态
          </div>
          {[
            ['门控', `${gv.icon} ${gv.label}`],
            ['仅 Wi-Fi', status.wifi_only ? '开' : '关'],
            ['离线队列积压', `${status.queue_len} 条`],
            ['待处理冲突', `副本 ${status.pending_artifacts} · 语义 ${status.pending_semantic}`],
          ].map(([k, v]) => (
            <div
              key={k}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                gap: tokens.spacing.sm,
                padding: `${tokens.spacing.xs}px 0`,
                fontSize: tokens.typography.body.size,
              }}
            >
              <span style={{ color: tokens.color.textSecondary }}>{k}</span>
              <span>{v}</span>
            </div>
          ))}
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: tokens.spacing.sm,
              marginTop: tokens.spacing.md,
            }}
          >
            <button
              type="button"
              onClick={() => {
                setPopupOpen(false);
                onOpenQueue();
              }}
              style={entryBtnStyle}
            >
              打开离线队列面板
            </button>
            <button
              type="button"
              onClick={() => {
                setPopupOpen(false);
                onOpenConflicts();
              }}
              style={entryBtnStyle}
            >
              打开冲突处理面板
            </button>
            <button
              type="button"
              onClick={onToggleWifiOnly}
              disabled={wifiOnly === null}
              title={wifiOnly === null ? '开关状态不可用' : undefined}
              style={{
                ...entryBtnStyle,
                opacity: wifiOnly === null ? 0.5 : 1,
                cursor: wifiOnly === null ? 'not-allowed' : 'pointer',
              }}
            >
              仅 Wi-Fi 同步：{wifiOnly === null ? '—' : wifiOnly ? '开（点击关闭）' : '关（点击开启）'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

const entryBtnStyle: React.CSSProperties = {
  textAlign: 'left',
  background: tokens.color.bgElevated,
  border: '1px solid rgba(255,255,255,0.10)',
  borderRadius: tokens.radius.md,
  padding: `${tokens.spacing.sm + 2}px ${tokens.spacing.md}px`,
  color: tokens.color.textPrimary,
  fontSize: tokens.typography.body.size,
  cursor: 'pointer',
};
