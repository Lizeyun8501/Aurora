/**
 * DiaryView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, listNotesTitled, type Task } from '../components/primitives';

export function DiaryView({ onOpen, showToast }: {
  onOpen: (id: string, title: string) => void;
  showToast: (m: string) => void;
}) {
  const [monthOffset, setMonthOffset] = useState(0);
  const diaries = listNotesTitled()
    .filter((n) => /日记/.test(n.title))
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));

  const now = new Date();
  const view = new Date(now.getFullYear(), now.getMonth() + monthOffset, 1);
  const y = view.getFullYear();
  const m = view.getMonth();
  const firstDay = new Date(y, m, 1).getDay(); // 0=周日
  const lead = (firstDay + 6) % 7; // 周一起始
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  const fmt = (d: number) => `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
  const dayHas = (d: number) => diaries.some((n) => n.title.includes(fmt(d)));
  const todayStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;

  const writeToday = () => {
    const title = `日记 ${todayStr}`;
    const exist = diaries.find((n) => n.title.includes(todayStr));
    if (exist) { onOpen(exist.id, exist.title); return; }
    const id = platform.createNote(title);
    if (!id) return;
    showToast('开始写今天的日记');
    onOpen(id, title);
  };

  return (
    <div className="view">
      <div className="card">
        <div className="cal-head">
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o - 1)} aria-label="上月">{I.back}</button>
          <span className="cal-title">{y} 年 {m + 1} 月</span>
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o + 1)} aria-label="下月">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m9 18 6-6-6-6"/></svg>
          </button>
        </div>
        <div className="cal-grid">
          {['一', '二', '三', '四', '五', '六', '日'].map((w) => (
            <span key={w} className="cal-week">{w}</span>
          ))}
          {Array.from({ length: lead }, (_, i) => <span key={`e${i}`} />)}
          {Array.from({ length: daysInMonth }, (_, i) => {
            const d = i + 1;
            const isToday = fmt(d) === todayStr;
            const has = dayHas(d);
            return (
              <span key={d} className={`cal-day ${isToday ? 'today' : ''} ${has ? 'has' : ''}`}>
                {d}
              </span>
            );
          })}
        </div>
        <button className="btn btn-primary btn-block" style={{ marginTop: 8 }} onClick={writeToday}>
          {I.diaryPen} {diaries.some((n) => n.title.includes(todayStr)) ? '继续写今天日记' : '写今日日记'}
        </button>
      </div>

      <div className="result-section-title" style={{ marginTop: 12 }}>全部日记 · {diaries.length}</div>
      {diaries.length === 0 ? (
        <EmptyState text="写第一篇日记，记录今天的心情与思考。" />
      ) : (
        <div className="note-list">
          {diaries.map((n) => (
            <NoteCard key={n.id} note={n} onOpen={onOpen} />
          ))}
        </div>
      )}
    </div>
  );
}
