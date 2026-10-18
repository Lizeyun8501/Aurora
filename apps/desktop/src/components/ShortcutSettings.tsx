/**
 * DK-2.10 快捷键设置面板（v1）— 键位列表+录入门（ShortcutSettings modal）。
 *
 * - 数据：启动/打开时 cmd_get_shortcuts 拉覆盖表，与 DEFAULT_SHORTCUTS
 *   合并呈现（override 优先）；保存=cmd_set_shortcuts 全量终态快照；
 * - 录入：捕获态 keydown——eventToCombo 规范化（无 Mod 拒绝+防单键劫持
 *   +修饰键忽略）+Esc 取消；录完即时本地预览，「保存」统一提交；
 * - 边界：无冲突检测（后写覆盖先写——诚实注记）；invoke=null 只读。
 */
import React, { useEffect, useState } from 'react';
import tokens from '../design/tokens';
import {
  DEFAULT_SHORTCUTS,
  SHORTCUT_LABELS,
  SHORTCUT_ACTIONS,
  eventToCombo,
  type ShortcutTable,
} from '../shortcuts';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

export default function ShortcutSettings(props: {
  invoke: InvokeFn | null;
  onClose: () => void;
}): React.ReactElement {
  const { invoke, onClose } = props;
  const [table, setTable] = useState<ShortcutTable>(DEFAULT_SHORTCUTS);
  const [capture, setCapture] = useState<string | null>(null); // 正在录入的 action
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!invoke) return;
    invoke('cmd_get_shortcuts')
      .then((v) => {
        if (v && typeof v === 'object') {
          setTable({ ...DEFAULT_SHORTCUTS, ...(v as ShortcutTable) });
        }
      })
      .catch(() => {
        /* 读失败走默认——设置面非关键路径 */
      });
  }, [invoke]);

  useEffect(() => {
    if (!capture) return;
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape') {
        setCapture(null);
        return;
      }
      const combo = eventToCombo(e);
      if (combo) {
        setTable((t) => ({ ...t, [capture]: combo }));
        setCapture(null);
      }
      // 无 Mod 组合：忽略继续等（提示见行内文案）
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [capture]);

  const save = (next: ShortcutTable): void => {
    setTable(next);
    if (!invoke) return;
    invoke('cmd_set_shortcuts', { table: next })
      .then(() => setMsg('已保存，即时生效'))
      .catch(() => setMsg('保存失败（已降级）'));
  };

  return (
    <div
      role="dialog"
      aria-label="快捷键设置"
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.5)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 90,
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        style={{
          background: tokens.color.bgSurface,
          border: '1px solid rgba(255,255,255,0.12)',
          borderRadius: tokens.radius.lg,
          padding: tokens.spacing.lg,
          minWidth: 380,
          maxWidth: 460,
        }}
      >
        <h2 style={{ margin: `0 0 ${tokens.spacing.sm}px`, fontSize: tokens.typography.heading.size }}>
          快捷键设置
        </h2>
        <p style={{ margin: `0 0 ${tokens.spacing.sm}px`, fontSize: 12, color: tokens.color.textSecondary }}>
          点击「改」后按下新组合键（须含 Mod/Control 或 ⌘；Esc 取消）。同组合后写覆盖先写。
        </p>
        <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
          {SHORTCUT_ACTIONS.map((action) => (
            <li
              key={action}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: tokens.spacing.sm,
                padding: '8px 4px',
                borderBottom: '1px solid rgba(255,255,255,0.06)',
              }}
            >
              <span style={{ fontSize: tokens.typography.caption.size + 1 }}>
                {SHORTCUT_LABELS[action] ?? action}
              </span>
              {capture === action ? (
                <kbd
                  style={{
                    fontSize: 12,
                    padding: '3px 10px',
                    borderRadius: tokens.radius.sm,
                    border: `1px dashed ${tokens.color.primaryBright}`,
                    color: tokens.color.primaryBright,
                  }}
                >
                  按下新组合键…（Esc 取消）
                </kbd>
              ) : (
                <>
                  <kbd
                    style={{
                      fontSize: 12,
                      padding: '3px 10px',
                      borderRadius: tokens.radius.sm,
                      border: '1px solid rgba(255,255,255,0.15)',
                      background: 'rgba(255,255,255,0.05)',
                    }}
                  >
                    {table[action] ?? DEFAULT_SHORTCUTS[action]}
                  </kbd>
                  <button
                    onClick={() => setCapture(action)}
                    style={{
                      border: '1px solid rgba(255,255,255,0.15)',
                      background: 'transparent',
                      color: tokens.color.textSecondary,
                      borderRadius: tokens.radius.sm,
                      fontSize: 11,
                      padding: '2px 8px',
                      cursor: 'pointer',
                    }}
                  >
                    改
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
        <div style={{ display: 'flex', gap: tokens.spacing.sm, marginTop: tokens.spacing.md, alignItems: 'center' }}>
          <button
            onClick={() => save({})}
            style={{
              border: '1px solid rgba(255,255,255,0.15)',
              background: 'transparent',
              color: tokens.color.textSecondary,
              borderRadius: tokens.radius.sm,
              fontSize: 12,
              padding: '5px 12px',
              cursor: 'pointer',
            }}
          >
            恢复默认
          </button>
          <button
            onClick={() => save(table)}
            style={{
              border: 'none',
              background: tokens.color.primary,
              color: '#fff',
              borderRadius: tokens.radius.sm,
              fontSize: 12,
              padding: '5px 14px',
              cursor: 'pointer',
            }}
          >
            保存
          </button>
          {msg && <span style={{ fontSize: 11, color: tokens.color.success }}>{msg}</span>}
          <span style={{ flex: 1 }} />
          <button
            onClick={onClose}
            style={{
              border: 'none',
              background: 'transparent',
              color: tokens.color.textSecondary,
              cursor: 'pointer',
              fontSize: 12,
            }}
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
