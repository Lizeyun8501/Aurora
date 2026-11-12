/**
 * AgentView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, type Task } from '../components/primitives';

export function AgentView({ theme }: { theme: string }) {
  void theme;
  /* B-01: 三态显式化——idle(空+引导) / running(进度+轮询) / failed(错误+重试)。
   * 禁任何分支伪造 agent 回复（DoD: 无模型时对话区保持空）。 */
  type ChatMsg = {
    role: 'user' | 'agent' | 'meta' | 'error' | 'tools';
    text: string;
    /** tools: {name, args} JSON 串数组（折叠时间轴明细）。 */
    tools?: string[];
  };
  const [msgs, setMsgs] = useState<ChatMsg[]>([]);
  const [input, setInput] = useState('');
  const [phase, setPhase] = useState<'idle' | 'running' | 'failed'>('idle');
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [remaining, setRemaining] = useState<number | null>(null);
  const [needSetup, setNeedSetup] = useState(false);
  const [lastQuery, setLastQuery] = useState('');
  /** 工具调用计数（首屏「调用了 N 个工具」——折叠明细见 tools 消息）。 */
  const [toolCount, setToolCount] = useState(0);
  /** liquify 提案（AI 结果先提案,勾选后才 commit——AI 永不静默写入）。 */
  const [proposals, setProposals] = useState<Array<{
    id: string; status: string; ops: Array<{ id: string; description: string }>;
  }>>([]);
  const [checkedOps, setCheckedOps] = useState<Record<string, number[]>>({});

  const bridge = platform as unknown as {
    agentSessionCreate?: () => string;
    agentSessionStatus?: (id: string) => string;
    agentSessionKill?: (id: string, reason: string) => boolean;
    agentChat?: (id: string, msgsJson: string) => string;
    aiLiquifyList?: () => string;
    aiLiquifyCommit?: (pid: string, selectedJson: string) => string;
  };

  /* 会话建立+15 分钟倒计时轮询（过期→重新授权 DoD;未配桥=mock 空态）。 */
  useEffect(() => {
    const raw = bridge.agentSessionCreate?.();
    if (!raw) return; // browser-mock/桥未挂——保持 idle 空态（不伪造会话）
    try {
      const sess = JSON.parse(raw) as { id: string; deadline_secs: number };
      setSessionId(sess.id);
      const tick = () => {
        try {
          const st = JSON.parse(bridge.agentSessionStatus?.(sess.id) ?? '{}') as {
            killed: boolean; expired: boolean; remaining_secs: number;
          };
          if (st.killed || st.expired) {
            setSessionId(null);
            setPhase('idle');
            setMsgs((m) => [...m, { role: 'meta', text: '会话已结束（过期/终止）——发送新消息需重新授权' }]);
            return;
          }
          setRemaining(st.remaining_secs);
        } catch { /* 轮询失败忽略——下轮再试 */ }
      };
      tick();
      const timer = setInterval(tick, 30_000);
      return () => clearInterval(timer);
    } catch { return; }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* liquify 提案刷新（提案卡区——复用桌面 LiquifyReview 语义）。 */
  const refreshProposals = useCallback(() => {
    const raw = bridge.aiLiquifyList?.();
    if (!raw) return;
    try {
      const parts = raw.split('\n__SEP__\n').filter(Boolean);
      const list = parts
        .map((x) => {
          try { return JSON.parse(x) as { id: string; status: string; ops?: Array<{ id: string; description: string }> }; }
          catch { return null; }
        })
        .filter(Boolean) as Array<{ id: string; status: string; ops?: Array<{ id: string; description: string }> }>;
      setProposals(list.map((x) => ({ id: x.id, status: x.status, ops: x.ops ?? [] })));
    } catch { /* 损坏 JSON——空态 */ }
  }, []);
  useEffect(refreshProposals, [refreshProposals]);

  const send = () => {
    const t = input.trim();
    if (!t || !sessionId || phase === 'running') return;
    setInput('');
    setLastQuery(t);
    setMsgs((m) => [...m, { role: 'user', text: t }]);
    setPhase('running');
    setNeedSetup(false);
    /* 工具时间轴（v1: 推理无工具明细时 0——首屏「调用了 N 个工具」计数诚实） */
    setToolCount(0);

    const messages = [
      { role: 'system', content: '你是 Aurora Note 的本地笔记助手,回答简洁准确。' },
      ...msgs.filter((m) => m.role === 'user' || m.role === 'agent').map((m) => ({
        role: m.role === 'user' ? 'user' : 'assistant',
        content: m.text,
      })),
      { role: 'user', content: t },
    ];
    /* 同步桥真调（JS 层后台线程化不可行——JSCore 单线程;进度态=running UI
     * 即时呈现,轮询 session 倒计时驱动进度可读面）。 */
    const result = bridge.agentChat?.(sessionId, JSON.stringify(messages)) ?? 'ERR:CORE_NOT_INIT';
    setPhase('idle');

    if (result.startsWith('ERR:')) {
      const kind = result.slice(5);
      if (kind === 'SESSION_EXPIRED' || kind === 'SESSION_KILLED' || kind === 'SESSION_NOT_FOUND') {
        setSessionId(null);
        setMsgs((m) => [...m, { role: 'meta', text: '会话已结束（15 分钟限时/被终止）——请发送新消息重新授权' }]);
      } else if (kind === 'CORE_NOT_INIT' || kind.includes('unavailable') || kind.includes('No AI provider')
                 || kind.includes('connection') || kind.includes('404') || kind.includes('refused')) {
        /* 真实失败=provider 不可达/未配置——对话区保持空+设置引导（可点击）,
         * 不伪造任何回复文案（B-01 红线）。 */
        setNeedSetup(true);
      } else {
        setMsgs((m) => [...m, { role: 'error', text: `推理失败（${kind}）——可重试` }]);
      }
      return;
    }
    if (!result.trim()) {
      setMsgs((m) => [...m, { role: 'error', text: '推理返回空——可重试' }]);
      return;
    }
    setMsgs((m) => [...m, { role: 'agent', text: result }]);
    /* AI 产出文本不直接写库（写=liquify 提案经用户勾选 commit——DoD）。 */
    refreshProposals();
  };

  const retry = () => {
    setInput(lastQuery);
    setMsgs((m) => m.filter((x) => x.role !== 'error'));
  };

  const commitProposal = (pid: string) => {
    const sel = checkedOps[pid] ?? [];
    const r = bridge.aiLiquifyCommit?.(pid, JSON.stringify(sel)) ?? 'ERR:CORE_NOT_INIT';
    if (r === 'ok') {
      setMsgs((m) => [...m, { role: 'meta', text: `提案 ${pid} 已确认落库（${sel.length} 项）` }]);
    } else {
      setMsgs((m) => [...m, { role: 'error', text: `提案提交失败（${r.startsWith('ERR:') ? r.slice(5) : r}）` }]);
    }
    refreshProposals();
  };

  return (
    <div className="view">
      {/* 权限与状态 — V19 §1.3;B-01: 状态真实化（倒计时/未建会话） */}
      <div className="card">
        <div className="card-row" style={{ marginBottom: 8 }}>
          <span className="sync-badge">
            <span className={sessionId ? 'sync-dot ok' : 'sync-dot off'} />
            {sessionId ? `⏱ 会话剩余 ${Math.floor((remaining ?? 0) / 60)} 分 ${(remaining ?? 0) % 60} 秒` : '会话未建立'}
          </span>
          <span style={{ flex: 1 }} />
          {sessionId && (
            <button className="btn btn-text" onClick={() => {
              bridge.agentSessionKill?.(sessionId, '用户手动终止');
              setSessionId(null);
            }}>终止会话</button>
          )}
          <span className="sync-badge">{I.lock} 私有空间</span>
        </div>
        <div className="settings-hint" style={{ padding: 0 }}>
          每次调用需二次确认 · 会话密钥限时自动过期（15 分钟） · 操作写入审计日志
        </div>
      </div>

      {/* B-01: 设置引导卡（无模型时——可点击跳设置;不伪装成回复） */}
      {needSetup && (
        <div className="card" role="alert" style={{ borderColor: 'var(--warn, #E8B33D)' }}>
          <div style={{ fontWeight: 600, marginBottom: 4 }}>AI 模型不可用</div>
          <div style={{ fontSize: 12, color: 'var(--muted)', marginBottom: 8 }}>
            本次对话未产生任何回复。需本地 Ollama（同网络）或在设置中配置云端模型后重试。
          </div>
          <button className="btn btn-primary" onClick={() => {
            /* goto 在 MobileApp 层——经 window 事件跳设置（同 drawer 导航语义） */
            window.dispatchEvent(new CustomEvent('aurora:goto', { detail: { view: 'settings' } }));
          }}>前往设置</button>
          <button className="btn btn-text" onClick={retry}>重试上一问</button>
        </div>
      )}

      {/* B-01 DoD: 消息列表 aria-live="polite"（读屏播报流式进度/新消息）。 */}
      <div className="agent-msgs" aria-live="polite" aria-label="Agent 对话消息">
        {phase === 'running' && (
          <div className="agent-msg agent" role="status">
            推理中…（本地优先）
          </div>
        )}
        {msgs.map((m, i) =>
          m.role === 'tools' ? (
            <details key={i} className="agent-msg agent" style={{ border: '1px solid var(--line)', borderRadius: 8 }}>
              <summary style={{ cursor: 'pointer' }}>调用了 {m.tools?.length ?? 0} 个工具</summary>
              <ul style={{ margin: '6px 0 0', paddingLeft: 16, fontSize: 12 }}>
                {(m.tools ?? []).map((tj, j) => {
                  try {
                    const t = JSON.parse(tj) as { name: string; args?: string };
                    return <li key={j}><b>{t.name}</b>{t.args ? `（${t.args}）` : ''}</li>;
                  } catch { return <li key={j}>{tj}</li>; }
                })}
              </ul>
            </details>
          ) : (
            <div key={i} className={`agent-msg ${m.role}`}>{m.text}</div>
          ),
        )}
      </div>

      {/* B-01: liquify 提案卡区（复用桌面 LiquifyReview 语义: 勾选→commit;
          AI 永不静默写入——未 commit 零写事件 DoD）。 */}
      {proposals.length > 0 && (
        <div className="card" aria-label="AI 提案确认">
          <div style={{ fontWeight: 600, marginBottom: 6 }}>AI 提案（{proposals.length}）</div>
          {proposals.map((p) => (
            <div key={p.id} style={{ borderBottom: '1px solid var(--line)', padding: '8px 0' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontSize: 12, flex: 1 }}>{p.id} · {p.status}</span>
                {p.status === 'Draft' && (
                  <button className="btn btn-primary" style={{ minHeight: 32 }} onClick={() => commitProposal(p.id)}>
                    确认落库
                  </button>
                )}
              </div>
              {p.ops.map((op, oi) => (
                <label key={op.id} style={{ display: 'flex', gap: 8, alignItems: 'center', padding: '4px 0', fontSize: 13 }}>
                  <input
                    type="checkbox"
                    aria-label={`提案项 ${op.description}`}
                    checked={((checkedOps[p.id] ?? []).includes(oi))}
                    onChange={(e) => {
                      const cur = checkedOps[p.id] ?? [];
                      setCheckedOps((prev) => ({
                        ...prev,
                        [p.id]: e.target.checked ? [...cur, oi] : cur.filter((x) => x !== oi),
                      }));
                    }}
                  />
                  <span>{op.description}</span>
                </label>
              ))}
            </div>
          ))}
        </div>
      )}

      <div className="agent-input-row">
        <input
          className="input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && send()}
          placeholder={sessionId ? '向 Agent 提问…' : '会话未建立…'}
          aria-label="向 Agent 提问"
          disabled={!sessionId}
        />
        <button className="btn btn-primary" onClick={send} disabled={!sessionId || phase === 'running'}>发送</button>
      </div>
    </div>
  );
}
