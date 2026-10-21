/**
 * DK-3.3：全局快速捕获浮层 — 系统级快捷键（Alt+Shift+N）触发弹速记窗。
 *
 * 链路：Rust global-shortcut 插件 emit `quick-capture` 事件 → 本浮层弹起
 * → textarea 速记 → cmd_import_share（SharePack 复用——与 2.9 剪贴板
 * 捕获同链）落库+#capture 标签（首行截断 title 沿用 v1 语义）。
 *
 * R3-1：NL 日期实时预览——输入 debounce 250ms invoke `cmd_preview_due`
 * （core `due_parse` 单一权威），命中片段 chip 标注（「📅 明天 09:00 ·
 * 已识别」）；v1 边界：chip 预览不做框内 overlay 高亮（字节 span 已就绪，
 * overlay 是 v2 感知升级项）；browser-mock 静默降级。
 *
 * v1 边界：固定键 Alt+Shift+N（系统级键与应用内快捷键表域不同，v2 打通）；
 * 全局键注册失败降级（palette「快速捕获」入口同链可用）。
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import tokens from '../design/tokens';
import type { InvokeFn } from './DesktopShell';

export const QUICK_CAPTURE_EVENT = 'quick-capture';

/** R3-1：cmd_preview_due 返回面（core DueParse span + human）。 */
export interface DuePreview {
  due_ts: number;
  start: number;
  end: number;
  human: string;
}

interface Props {
  invoke: InvokeFn;
  onClose: () => void;
}

export default function QuickCapture({ invoke, onClose }: Props) {
  const [text, setText] = useState('');
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<DuePreview | null>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);

  // 挂载即聚焦（速记场景零摩擦）
  useEffect(() => {
    taRef.current?.focus();
  }, []);

  // R3-1：NL 日期实时预览（debounce 250ms；空串即清；invoke 失败静默——预览非权威）
  useEffect(() => {
    if (!text.trim()) {
      setPreview(null);
      return;
    }
    const timer = setTimeout(() => {
      invoke('cmd_preview_due', { text })
        .then((r) => setPreview((r as DuePreview | null) ?? null))
        .catch(() => setPreview(null)); // mock/降级静默
    }, 250);
    return () => clearTimeout(timer);
  }, [text, invoke]);

  const save = useCallback(async () => {
    const t = text.trim();
    if (!t || busy) return;
    setBusy(true);
    setErr(null);
    const firstLine = t.split('\n')[0].trim().slice(0, 60) || '快速捕获';
    try {
      const r = (await invoke('cmd_import_share', {
        code: btoa(
          unescape(
            encodeURIComponent(
              JSON.stringify({
                title: firstLine,
                content: t,
                createdAt: new Date().toISOString(),
              }),
            ),
          ),
        ),
      })) as { note_id?: string };
      if (r.note_id) {
        // #capture 标签（追加语义——失败静默不阻断）
        invoke('cmd_set_note_tags', { noteId: r.note_id, tags: ['capture'] }).catch(() => {});
        setMsg('已捕获（#capture）');
        setTimeout(onClose, 500);
      } else {
        setErr('捕获失败：无返回 note_id');
      }
    } catch (e) {
      setErr(`捕获失败：${String(e)}`);
    } finally {
      setBusy(false);
    }
  }, [text, busy, invoke, onClose]);

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(8, 12, 18, 0.6)',
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'center',
        paddingTop: '18vh',
        zIndex: 2100,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: tokens.color.bgSurface,
          border: '1px solid rgba(255,255,255,0.12)',
          borderRadius: tokens.radius.md,
          padding: tokens.spacing.md,
          width: 560,
          maxWidth: '92vw',
        }}
      >
        <div style={{ fontSize: 13, color: tokens.color.textSecondary, marginBottom: 6 }}>
          快速捕获{' '}
          <span style={{ color: tokens.color.textDisabled }}>
            （Alt+Shift+N 唤起 · Cmd/Ctrl+Enter 保存 · Esc 关闭 · 落 #capture）
          </span>
        </div>
        <textarea
          ref={taRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void save();
            }
            if (e.key === 'Escape') onClose();
          }}
          rows={4}
          placeholder="想到什么写什么——第一行作标题"
          style={{
            width: '96%',
            fontFamily: tokens.typography.mono,
            fontSize: 13,
            background: tokens.color.bgBase,
            color: tokens.color.textPrimary,
            border: '1px solid rgba(255,255,255,0.16)',
            borderRadius: tokens.radius.sm,
            padding: 8,
            resize: 'vertical',
          }}
        />
        {err && (
          <div role="alert" style={{ color: '#f88', fontSize: 13, marginTop: 4 }}>
            {err}
          </div>
        )}
        {/* R3-1：NL 日期 chip——命中片段 human 标注（识别失败不显示） */}
        {preview && (
          <div
            data-testid="due-preview-chip"
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 6,
              marginTop: 6,
              padding: '3px 10px',
              borderRadius: tokens.radius.sm,
              background: 'rgba(88,166,255,0.14)',
              border: '1px solid rgba(88,166,255,0.35)',
              color: '#8ab8ff',
              fontSize: 12,
            }}
          >
            <span aria-hidden>📅</span>
            <span style={{ fontWeight: 600 }}>{preview.human}</span>
            <span style={{ color: 'rgba(138,184,255,0.7)' }}>· 已识别截止时间</span>
          </div>
        )}
        {msg && (
          <div style={{ color: tokens.color.primaryBright, fontSize: 13, marginTop: 4 }}>{msg}</div>
        )}
        <div style={{ textAlign: 'right', marginTop: 6 }}>
          <button
            disabled={busy || !text.trim()}
            onClick={() => void save()}
            style={{
              background: tokens.color.primary,
              border: 'none',
              borderRadius: tokens.radius.sm,
              color: tokens.color.textPrimary,
              cursor: busy || !text.trim() ? 'not-allowed' : 'pointer',
              padding: '4px 14px',
              fontSize: 13,
            }}
          >
            {busy ? '…' : '捕获'}
          </button>
        </div>
      </div>
    </div>
  );
}

/** 事件监听挂载（DesktopShell 用——mock/browser-mock 环境静默降级）。 */
export function useQuickCaptureEvent(invoke: InvokeFn | null, onOpen: () => void): void {
  useEffect(() => {
    if (!invoke) return; // mock 模式无 tauri 事件面
    let un: (() => void) | undefined;
    listen(QUICK_CAPTURE_EVENT, () => onOpen())
      .then((f) => {
        un = f as unknown as () => void;
      })
      .catch(() => {}); // 非 tauri 环境（browser-mock）静默
    return () => un?.();
  }, [invoke, onOpen]);
}
