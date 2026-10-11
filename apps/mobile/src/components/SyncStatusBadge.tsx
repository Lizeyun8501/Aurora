/**
 * SyncStatusBadge — DK-50 组件 1：移动端常驻同步指示（移植自桌面 DK-48）
 *
 * 逻辑层照搬（15s 轮询 / gate 三态映射 / 冲突红点 / sheet 三入口）；
 * 样式层改写为 mobile.css 体系（sync-* 前缀，sync-dot 族复用——颜色
 * 不作唯一载体，点+文字+数字三通道）。
 *
 * 数据源：DK-49 platform 面（syncStatus/getWifiOnly/setWifiOnly——
 * mock 回放路径可用：无 AndroidBridge 环境走 androidPlatform 内存态）。
 *
 * 功耗裁决（工单 §2/DoD3）：visibilitychange 不可见时跳过轮询请求、
 * 恢复可见立即刷新一次——切后台零请求，前台 15s 节拍不变。
 */
import { useCallback, useEffect, useState } from 'react';
import { platform, type SyncStatus } from '../adapters/androidPlatform';

interface Props {
  onOpenQueue: () => void;
  onOpenConflicts: () => void;
}

const POLL_MS = 15_000;

function gateLabel(gate: string): string {
  switch (gate) {
    case 'Allow':
      return '同步就绪';
    case 'DeferUntilUnmetered':
      return '待 Wi-Fi';
    case 'DeferUntilOnline':
      return '离线';
    default:
      return gate;
  }
}

function gateDotClass(gate: string): string {
  switch (gate) {
    case 'Allow':
      return 'sync-dot ok';
    case 'DeferUntilUnmetered':
      return 'sync-dot warn';
    default:
      return 'sync-dot off';
  }
}

export default function SyncStatusBadge({ onOpenQueue, onOpenConflicts }: Props) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [wifiOnly, setWifiOnly] = useState<boolean | null>(null);
  const [toggling, setToggling] = useState(false);

  const refresh = useCallback(() => {
    try {
      setStatus(platform.syncStatus());
      setWifiOnly(platform.getWifiOnly());
    } catch {
      /* mock/桥异常：保持上次状态，badge 显示离线缺省 */
    }
  }, []);

  useEffect(() => {
    refresh();
    // DK-50 功耗裁决：interval 回调内检查可见性——切后台零请求；
    // 恢复可见（visibilitychange → visible）立即刷新一次。
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') refresh();
    }, POLL_MS);
    const onVisible = () => {
      if (document.visibilityState === 'visible') refresh();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [refresh]);

  const toggleWifi = useCallback(() => {
    if (wifiOnly === null || toggling) return;
    setToggling(true);
    try {
      platform.setWifiOnly(!wifiOnly);
      setWifiOnly(!wifiOnly);
    } catch {
      /* 设置失败保持原值——不静默改观感 */
    } finally {
      setToggling(false);
    }
  }, [wifiOnly, toggling]);

  if (!status) return null;

  const pendingConflicts = status.pending_artifacts + status.pending_semantic;

  return (
    <>
      <button
        type="button"
        className="sync-gate-badge"
        onClick={() => {
          refresh();
          setSheetOpen(true);
        }}
        aria-label={`同步状态：${gateLabel(status.gate)}，队列 ${status.queue_len} 条，冲突 ${pendingConflicts} 条`}
      >
        <span className={gateDotClass(status.gate)} />
        {status.queue_len > 0 && <span className="sync-gate-num">{status.queue_len}</span>}
        {pendingConflicts > 0 && <span className="sync-red-dot" />}
      </button>

      {sheetOpen && (
        <>
          <div className="sync-sheet-mask" onClick={() => setSheetOpen(false)} />
          <div className="sync-sheet" role="dialog" aria-label="同步状态详情">
            <div className="sync-sheet-grip" />
            <div className="sync-sheet-title">
              同步状态
              <button type="button" className="sync-sheet-close" onClick={() => setSheetOpen(false)}>
                关闭
              </button>
            </div>
            <div className="sync-row-line">
              <span>门控判定</span>
              <b>
                <span className={gateDotClass(status.gate)} style={{ display: 'inline-block', marginRight: 6 }} />
                {gateLabel(status.gate)}
              </b>
            </div>
            <div className="sync-row-line">
              <span>仅 Wi-Fi 同步</span>
              <b>{status.wifi_only ? '开' : '关'}</b>
            </div>
            <div className="sync-row-line">
              <span>离线队列积压</span>
              <b>{status.queue_len} 条</b>
            </div>
            <div className="sync-row-line">
              <span>待处理冲突</span>
              <b>
                副本 {status.pending_artifacts} · 语义 {status.pending_semantic}
              </b>
            </div>
            <div className="sync-btn-row">
              <button
                type="button"
                className="sync-btn"
                onClick={() => {
                  setSheetOpen(false);
                  onOpenQueue();
                }}
              >
                队列面板
              </button>
              <button
                type="button"
                className="sync-btn"
                onClick={() => {
                  setSheetOpen(false);
                  onOpenConflicts();
                }}
              >
                冲突处理
              </button>
            </div>
            <div className="sync-btn-row">
              <button
                type="button"
                className="sync-btn"
                onClick={toggleWifi}
                disabled={wifiOnly === null || toggling}
              >
                仅 Wi-Fi 同步：{wifiOnly === null ? '不可用' : wifiOnly ? '开（点击关闭）' : '关（点击开启）'}
              </button>
            </div>
          </div>
        </>
      )}
    </>
  );
}
