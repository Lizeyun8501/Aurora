/**
 * DK-44：P2P 设备配对浮层 — 手动配对码模型（DK-40b 带外交换 UI 化）。
 *
 * 流程：双方各自「启用 P2P」→ 复制本机配对码带外互发 → 粘贴导入对端码
 * → 状态区刷新（已配对）。收件循环现场重接线（后端 cmd_pair_enable 内），
 * 直传/补发立即可用，无需重启。
 *
 * 诚实边界（v1）：配对码含 relay+直连地址（网络拓扑敏感——带外通道由
 * 用户自行选择）；relay 未就绪时导出的码仅局域网可达（提示条明示）。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';
import type { InvokeFn } from './DesktopShell';

interface PairStatus {
  p2p_enabled: boolean;
  peer_id: string;
  paired: boolean;
  local_code: string;
  relay_ready: boolean;
}

interface Props {
  invoke: InvokeFn;
  onClose: () => void;
}

export default function PairDialog({ invoke, onClose }: Props) {
  const [status, setStatus] = useState<PairStatus | null>(null);
  const [importCode, setImportCode] = useState('');
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    invoke('cmd_pair_status')
      .then((s) => setStatus(s as PairStatus))
      .catch((e) => setErr(String(e)));
  }, [invoke]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const enable = useCallback(() => {
    setBusy(true);
    setErr(null);
    invoke('cmd_pair_enable')
      .then((s) => {
        setStatus(s as PairStatus);
        setBusy(false);
      })
      .catch((e) => {
        setErr(String(e));
        setBusy(false);
      });
  }, [invoke]);

  const doImport = useCallback(() => {
    if (!importCode.trim()) return;
    setBusy(true);
    setErr(null);
    setMsg(null);
    invoke('cmd_pair_import', { code: importCode.trim() })
      .then(() => {
        setImportCode('');
        setMsg('对端地址已导入——补发/直传/收件已就绪');
        setBusy(false);
        refresh();
      })
      .catch((e) => {
        setErr(String(e));
        setBusy(false);
      });
  }, [invoke, importCode, refresh]);

  const unpair = useCallback(() => {
    if (!window.confirm('解除配对？对端地址将被删除（可重新导入）。')) return;
    setBusy(true);
    invoke('cmd_pair_unpair')
      .then(() => {
        setMsg('已解除配对');
        setBusy(false);
        refresh();
      })
      .catch((e) => {
        setErr(String(e));
        setBusy(false);
      });
  }, [invoke, refresh]);

  const copyCode = useCallback(() => {
    if (!status?.local_code) return;
    navigator.clipboard
      ?.writeText(status.local_code)
      .then(() => setMsg('本机配对码已复制——发给对方粘贴导入'))
      .catch(() => setErr('剪贴板写入失败——请手动复制'));
  }, [status]);

  const rowStyle = { marginBottom: tokens.spacing.md } as const;

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(8, 12, 18, 0.72)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1200,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="P2P 设备配对"
        style={{
          background: tokens.color.bgElevated,
          borderRadius: tokens.radius.md,
          padding: tokens.spacing.lg,
          width: 520,
          maxHeight: '80vh',
          overflowY: 'auto',
          color: tokens.color.textPrimary,
        }}
      >
        <div style={{ fontWeight: 700, marginBottom: tokens.spacing.md, fontSize: 15 }}>
          P2P 设备配对
        </div>

        {/* 状态区 */}
        <div style={rowStyle} data-testid="pair-status">
          状态：
          {status?.p2p_enabled ? (
            <>
              已启用{status.paired ? ' · 已配对 ✅' : ' · 未配对（导入对端码后可同步）'}
              {status.p2p_enabled && !status.relay_ready && status.local_code && (
                <span style={{ color: tokens.color.textSecondary }}>
                  {' '}
                  （relay 未连——码仅局域网可达）
                </span>
              )}
            </>
          ) : (
            '未启用'
          )}
        </div>

        {/* 启用+导出 */}
        <div style={rowStyle}>
          {status?.p2p_enabled ? (
            <>
              <button onClick={copyCode} style={btnStyle}>
                复制本机配对码
              </button>
              <code
                style={{
                  fontFamily: tokens.typography.mono,
                  fontSize: 11,
                  color: tokens.color.textSecondary,
                  display: 'block',
                  marginTop: 4,
                  wordBreak: 'break-all',
                }}
              >
                {status.local_code.slice(0, 72)}…
              </code>
            </>
          ) : (
            <button onClick={enable} disabled={busy} style={btnStyle}>
              启用 P2P 并生成本机配对码
            </button>
          )}
        </div>

        {/* 导入对端码 */}
        <div style={rowStyle}>
          <div style={{ marginBottom: 4, fontSize: 13 }}>粘贴对端配对码：</div>
          <textarea
            value={importCode}
            onChange={(e) => setImportCode(e.target.value)}
            rows={3}
            style={{
              width: '95%',
              fontFamily: tokens.typography.mono,
              fontSize: 12,
              background: tokens.color.bgBase,
              color: tokens.color.textPrimary,
              border: '1px solid rgba(255,255,255,0.16)',
              borderRadius: tokens.radius.sm,
              padding: 6,
            }}
          />
          <div style={{ marginTop: 6, display: 'flex', gap: 8 }}>
            <button onClick={doImport} disabled={busy || !importCode.trim()} style={btnStyle}>
              导入
            </button>
            {status?.paired && (
              <button onClick={unpair} disabled={busy} style={{ ...btnStyle, color: '#f88' }}>
                解除配对
              </button>
            )}
          </div>
        </div>

        {msg && (
          <div style={{ color: '#8f8', fontSize: 13, marginBottom: tokens.spacing.sm }}>{msg}</div>
        )}
        {err && (
          <div role="alert" style={{ color: '#f88', fontSize: 13, marginBottom: tokens.spacing.sm }}>
            {err}
          </div>
        )}

        <div
          style={{
            marginTop: tokens.spacing.md,
            paddingTop: tokens.spacing.sm,
            borderTop: '1px solid rgba(255,255,255,0.08)',
            color: tokens.color.textDisabled,
            fontSize: tokens.typography.caption.size,
          }}
        >
          配对码含网络地址——仅通过可信渠道交换。收件循环导入即生效，无需重启。
        </div>

        <div style={{ textAlign: 'right', marginTop: tokens.spacing.sm }}>
          <button onClick={onClose} style={btnStyle}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

const btnStyle: React.CSSProperties = {
  background: tokens.color.bgElevated,
  border: '1px solid rgba(255,255,255,0.2)',
  borderRadius: tokens.radius.sm,
  color: tokens.color.textPrimary,
  cursor: 'pointer',
  padding: '4px 12px',
  fontSize: 13,
};
