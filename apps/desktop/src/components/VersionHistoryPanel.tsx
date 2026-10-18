/**
 * DK-2.6 版本历史 v1 — 编辑器内容历史链面板（3.1 信息面板第四 tab）。
 *
 * - 数据：cmd_list_note_histories(noteId) → [{ts, size}]（升序，UI 倒序
 *   呈现）→ cmd_get_note_history(noteId, ts) 单版本明文预览；
 * - 对比：当前编辑内容 vs 选中历史——行级简易 diff（前后缀剥离+中间
 *   逐行 -/+ 标记；引号内 token 级/字级 diff 延后）；
 * - 回滚：确认门 → cmd_update_note(noteId, content)（全纪律写通路等价
 *   EditorPane saveContent 桥）→ 成功刷新预览；**不可自动撤销**——回滚
 *   前的当前内容会由保存链自动入历史（push_note_history），可再回滚回
 *   来（诚实边界：依赖保存链已触发）；
 * - 呈现纪律：文本原样渲染（不可信内容零 HTML interpret）；invoke=null
 *   （browser-mock）或失败 → 空态降级。
 */
import React, { useCallback, useEffect, useMemo, useState } from 'react';
import tokens from '../design/tokens';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
type HistoryEntry = { ts: number; size: number };

/** 行级简易 diff：前后缀剥离 + 中间逐行标记（- = 仅当前，+ = 仅历史，
 *  ' ' = 相同）。返回 [kind, text] 序列。 */
export function diffLines(cur: string, hist: string): [string, string][] {
  const a = cur.split('\n');
  const b = hist.split('\n');
  let s = 0;
  while (s < a.length && s < b.length && a[s] === b[s]) s += 1;
  let e = 0;
  while (
    e < a.length - s &&
    e < b.length - s &&
    a[a.length - 1 - e] === b[b.length - 1 - e]
  )
    e += 1;
  const out: [string, string][] = [];
  for (let i = s; i < a.length - e; i += 1) out.push(['-', a[i]]);
  for (let i = s; i < b.length - e; i += 1) out.push(['+', b[i]]);
  if (s > 0) out.unshift(['…', `（前 ${s} 行相同）`]);
  if (e > 0) out.push(['…', `（后 ${e} 行相同）`]);
  if (out.length === 0) out.push([' ', '（内容一致）']);
  return out;
}

const fmtTs = (ts: number): string => {
  try {
    return new Date(ts).toLocaleString();
  } catch {
    return String(ts);
  }
};

export default function VersionHistoryPanel(props: {
  invoke: InvokeFn | null;
  noteId: string;
  currentContent: string;
}): React.ReactElement {
  const { invoke, noteId, currentContent } = props;
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [selTs, setSelTs] = useState<number | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    const alive = { current: true };
    setEntries(null);
    setSelTs(null);
    setPreview(null);
    setMsg(null);
    if (!invoke) {
      setEntries([]);
      return () => {
        alive.current = false;
      };
    }
    invoke('cmd_list_note_histories', { noteId })
      .then((rows) => {
        if (alive.current) setEntries(((rows as HistoryEntry[]) ?? []).slice().reverse());
      })
      .catch(() => {
        if (alive.current) {
          setEntries([]);
          setFailed(true);
        }
      });
    return () => {
      alive.current = false;
    };
  }, [invoke, noteId]);

  const loadPreview = useCallback(
    (ts: number): void => {
      if (!invoke) return;
      setSelTs(ts);
      setPreview(null);
      invoke('cmd_get_note_history', { noteId, ts })
        .then((v) => setPreview(String(v ?? '')))
        .catch(() => setPreview(null));
    },
    [invoke, noteId],
  );

  const diff = useMemo(
    () => (preview === null ? null : diffLines(currentContent, preview)),
    [preview, currentContent],
  );

  const rollback = (): void => {
    if (!invoke || selTs === null || busy) return;
    if (!window.confirm('将当前内容替换为该历史版本？（当前内容会先入历史，可再回滚回来）')) return;
    setBusy(true);
    setMsg(null);
    invoke('cmd_update_note', { noteId, content: preview })
      .then(() => {
        setMsg('已回滚（保存链已入历史）');
        const alive = { current: true };
        invoke('cmd_list_note_histories', { noteId })
          .then((rows) => {
            if (alive.current) setEntries(((rows as HistoryEntry[]) ?? []).slice().reverse());
          })
          .catch(() => undefined);
      })
      .catch(() => setMsg('回滚失败（详见通知/日志）'))
      .finally(() => setBusy(false));
  };

  const rowBtn = (t: HistoryEntry): React.ReactElement => (
    <li key={t.ts}>
      <button
        onClick={() => loadPreview(t.ts)}
        style={{
          width: '100%',
          textAlign: 'left',
          padding: '5px 6px',
          background: selTs === t.ts ? tokens.color.primary : 'transparent',
          border: `1px solid ${selTs === t.ts ? tokens.color.primaryBright : 'rgba(255,255,255,0.08)'}`,
          borderRadius: tokens.radius.sm,
          color: selTs === t.ts ? '#fff' : tokens.color.textSecondary,
          cursor: 'pointer',
          fontSize: 12,
        }}
      >
        {fmtTs(t.ts)} · {t.size}B
      </button>
    </li>
  );

  return (
    <section style={{ minHeight: 0 }}>
      <p style={{ margin: `0 0 6px`, color: tokens.color.textSecondary, fontSize: 12 }}>
        {failed
          ? '加载失败（已降级）'
          : entries === null
            ? '加载中…'
            : entries.length === 0
              ? '暂无历史版本（随保存自动累积）'
              : `${entries.length} 个历史版本（新→旧）`}
      </p>
      <ul style={{ listStyle: 'none', margin: `0 0 8px`, padding: 0, maxHeight: 150, overflowY: 'auto' }}>
        {(entries ?? []).map(rowBtn)}
      </ul>
      {selTs !== null && (
        <>
          <div style={{ display: 'flex', gap: 6, alignItems: 'center', margin: `0 0 6px` }}>
            <span style={{ fontSize: 11, color: tokens.color.textSecondary }}>
              对比：当前 ↔ {fmtTs(selTs)}
            </span>
            <button
              disabled={busy || preview === null}
              onClick={rollback}
              style={{
                padding: '3px 10px',
                fontSize: 12,
                borderRadius: tokens.radius.sm,
                border: `1px solid ${tokens.color.danger}`,
                background: busy ? 'transparent' : tokens.color.danger,
                color: busy ? tokens.color.textSecondary : '#fff',
                cursor: busy ? 'wait' : 'pointer',
              }}
            >
              {busy ? '回滚中…' : '回滚到该版本'}
            </button>
          </div>
          <pre
            style={{
              margin: 0,
              maxHeight: 200,
              overflowY: 'auto',
              fontSize: 11,
              lineHeight: 1.5,
              background: tokens.color.bgSurface,
              border: '1px solid rgba(255,255,255,0.08)',
              borderRadius: tokens.radius.sm,
              padding: tokens.spacing.xs,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}
          >
            {(diff ?? []).map(([kind, text], i) => (
              <div
                key={i}
                style={{
                  color:
                    kind === '-'
                      ? tokens.color.danger
                      : kind === '+'
                        ? tokens.color.success
                        : tokens.color.textSecondary,
                }}
              >
                {kind === '…' ? text : `${kind} ${text}`}
              </div>
            ))}
          </pre>
          {msg && (
            <p style={{ margin: `6px 0 0`, fontSize: 11, color: tokens.color.success }}>{msg}</p>
          )}
        </>
      )}
    </section>
  );
}
