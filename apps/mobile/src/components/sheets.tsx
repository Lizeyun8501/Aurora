/**
 * sheets — B-02 切片三: CaptureSheet/TaskEditSheet/ShareReceiveSheet 拆出（纯搬移）。
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, EmptyState, type Task, type TaskPriority, type TaskStatus, TASK_PRIO, TASK_GROUPS } from './primitives';
import type { SharedIntent, ShareTarget } from './intent';

/** A-06: 分享接收浮层（内容预览+目标切换——text 型 img 预览经 data URL）。 */
export function ShareReceiveSheet(props: {
  items: SharedIntent[];
  imageB64: string | null;
  onClose: () => void;
  onLand: (target: ShareTarget) => void;
}) {
  const { items, imageB64, onClose, onLand } = props;
  const [target, setTarget] = useState<ShareTarget>('daily');
  const text = items.filter((i) => i.kind === 'text').map((i) => i.content).join('\n---\n');
  const img = items.find((i) => i.kind === 'image');
  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet" role="dialog" aria-label="系统分享接收">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">📥 接收到 {items.length} 项分享</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          {text && <pre className="input" style={{ whiteSpace: 'pre-wrap', margin: '0 0 10px', maxHeight: 160, overflow: 'auto', fontSize: 13 }}>{text}</pre>}
          {img && (
            <div style={{ marginBottom: 10 }}>
              {imageB64
                ? <img src={imageB64} alt="分享图片预览" style={{ maxWidth: '100%', borderRadius: 8, border: '1px solid rgba(0,0,0,0.1)' }} />
                : <div style={{ fontSize: 12, opacity: 0.7 }}>🖼️ 图片已保存（{img.content}）</div>}
            </div>
          )}
          <div className="capture-modes" role="radiogroup" aria-label="落位目标">
            <button className={`capture-mode ${target === 'daily' ? 'active' : ''}`} role="radio" aria-checked={target === 'daily'} onClick={() => setTarget('daily')}>
              📔<span>今日日记（默认）</span>
            </button>
            <button className={`capture-mode ${target === 'newNote' ? 'active' : ''}`} role="radio" aria-checked={target === 'newNote'} onClick={() => setTarget('newNote')}>
              📄<span>新建笔记</span>
            </button>
          </div>
          <button className="btn btn-primary btn-block" onClick={() => onLand(target)}>落位</button>
          <button className="btn btn-secondary btn-block" style={{ marginTop: 8 }} onClick={onClose}>暂不处理</button>
        </div>
      </div>
    </>
  );
}

export function CaptureSheet({ onClose, onNewNote, onNewTask, onNewEvent }: {
  onClose: () => void;
  onNewNote: (title: string, body: string) => void;
  onNewTask: (text: string, status: TaskStatus, priority: TaskPriority) => void;
  /** DK-CALENDAR-02 任务 3：FAB 捕获流「日程」类型（今日 09:00 预填）。 */
  onNewEvent: (title: string) => void;
}) {
  const [mode, setMode] = useState<'text' | 'ocr' | 'voice'>('text');
  const [text, setText] = useState('');
  const [prio, setPrio] = useState<TaskPriority>('normal'); // E17: 捕获即定优先级

  // AI 预处理（本地启发式演示 — 推荐 PARA 分类 + 提取行动任务）
  const aiHint = (() => {
    const t = text.trim();
    if (!t) return null;
    const isAction = /(完成|处理|回复|提交|联系|整理|修复|部署|review|fix)/i.test(t);
    const category = isAction ? 'Projects' : 'Resources';
    return { category, isAction };
  })();

  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">快速捕获</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          <div className="capture-modes">
            <button className={`capture-mode ${mode === 'text' ? 'active' : ''}`} onClick={() => setMode('text')}>
              {I.text}<span>文字录入</span>
            </button>
            <button className={`capture-mode ${mode === 'ocr' ? 'active' : ''}`} onClick={() => setMode('ocr')}>
              {I.camera}<span>图片 OCR</span>
            </button>
            <button className={`capture-mode ${mode === 'voice' ? 'active' : ''}`} onClick={() => setMode('voice')}>
              {I.mic}<span>语音速记</span>
            </button>
          </div>

          {mode === 'text' && (
            <>
              {/* E17 任务中心增强: 捕获即定优先级（三段）——high 排前/色点可视 */}
              <div className="task-prio-row" role="radiogroup" aria-label="优先级">
                {TASK_PRIO.map((p) => (
                  <button key={p.id} className={`task-prio-seg ${prio === p.id ? 'on' : ''}`}
                    role="radio" aria-checked={prio === p.id} onClick={() => setPrio(p.id)}>
                    <span className="task-prio-dot" style={{ background: p.dot }} />{p.label}
                  </button>
                ))}
              </div>
              <textarea
                className="input"
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="想到什么写什么，AI 自动整理…"
                autoFocus
              />
              {aiHint && (
                <div className="capture-ai-tip">
                  🤖 AI 预处理建议 — 推荐分类 <b>{aiHint.category}</b>
                  {aiHint.isAction ? ' · 识别为行动任务，可加入今日' : ' · 适合归档为笔记'}
                </div>
              )}
              <div className="capture-actions">
                <button
                  className="btn btn-primary"
                  disabled={!text.trim()}
                  style={{ opacity: text.trim() ? 1 : 0.5 }}
                  onClick={() => {
                    const t = text.trim();
                    onNewNote(t.slice(0, 24), t);
                  }}
                >生成笔记</button>
                <button
                  className="btn btn-secondary"
                  disabled={!text.trim()}
                  style={{ opacity: text.trim() ? 1 : 0.5 }}
                  onClick={() => onNewTask(text.trim().slice(0, 60), 'next', prio)}
                >加入今日任务</button>
                <button
                  className="btn btn-secondary"
                  disabled={!text.trim()}
                  style={{ opacity: text.trim() ? 1 : 0.5 }}
                  onClick={() => onNewEvent(text.trim().slice(0, 60))}
                >存为日程</button>
              </div>
            </>
          )}
          {mode === 'ocr' && (
            <div className="empty-state">
              <div className="empty-text">图片 OCR 捕获即将上线<br />当前版本可先用文字录入</div>
            </div>
          )}
          {mode === 'voice' && (
            <div className="empty-state">
              <div className="empty-text">语音速记即将上线<br />当前版本可先用文字录入</div>
            </div>
          )}
        </div>
      </div>
    </>
  );
}

/** 11-09 07:31 用户令: 待办单击→可编辑弹窗（非跳日历）——文本/截止日期/优先级/状态+保存/删除。
    复用 sheet/task-prio-seg 现成样式（同 CaptureSheet 观感体系）。 */
export function TaskEditSheet({ task, onClose, onSave, onDelete }: {
  task: Task;
  onClose: () => void;
  onSave: (t: Task) => void;
  onDelete: (id: string) => void;
}) {
  const [text, setText] = useState(task.text);
  const [due, setDue] = useState(task.due ?? '');
  const [prio, setPrio] = useState<TaskPriority>(task.priority ?? 'normal');
  const [status, setStatus] = useState<TaskStatus>(task.status);
  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">编辑待办</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          <textarea className="input" rows={3} value={text} onChange={(e) => setText(e.target.value)} autoFocus />
          <div className="task-prio-row" role="radiogroup" aria-label="优先级">
            {TASK_PRIO.map((p) => (
              <button key={p.id} className={`task-prio-seg ${prio === p.id ? 'on' : ''}`} role="radio" aria-checked={prio === p.id}
                onClick={() => setPrio(p.id)}>
                <span className="task-prio-dot" style={{ background: p.dot }} />{p.label}
              </button>
            ))}
          </div>
          <div className="task-prio-row" role="radiogroup" aria-label="状态">
            {TASK_GROUPS.map((g) => (
              <button key={g.id} className={`task-prio-seg ${status === g.id ? 'on' : ''}`} role="radio" aria-checked={status === g.id}
                onClick={() => setStatus(g.id)}>{g.label}</button>
            ))}
          </div>
          <label style={{ display: 'flex', alignItems: 'center', gap: 10, margin: '10px 0 4px', fontSize: 13, color: 'var(--text-secondary)' }}>
            <span>截止日期</span>
            <input type="date" className="input" style={{ flex: 1 }} value={due} onChange={(e) => setDue(e.target.value)} />
          </label>
          <div className="capture-actions">
            <button className="btn btn-text" onClick={() => { onDelete(task.id); onClose(); }}>删除</button>
            <button className="btn btn-primary" disabled={!text.trim()} style={{ opacity: text.trim() ? 1 : 0.5 }}
              onClick={() => { onSave({ ...task, text: text.trim(), due: due || undefined, priority: prio, status }); onClose(); }}>保存</button>
          </div>
        </div>
      </div>
    </>
  );
}
