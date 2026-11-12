/**
 * SearchView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, type Task } from '../components/primitives';
import type { SearchResult } from '../adapters/androidPlatform';

export function SearchView({ onOpen, initialQuery }: { onOpen: (id: string, title: string) => void; initialQuery?: string }) {
  const [query, setQuery] = useState(initialQuery ?? '');
  useEffect(() => {
    if (initialQuery) setQuery(initialQuery);
  }, [initialQuery]);
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [history, setHistory] = useState<string[]>(() => {
    try { return JSON.parse(localStorage.getItem('aurora.searchHistory') ?? '[]'); } catch { return []; }
  });
  const [auditOpen, setAuditOpen] = useState(false);

  const doSearch = (q: string) => {
    const t = q.trim();
    if (!t) return;
    setResults(platform.searchNotes(t));
    const h = [t, ...history.filter((x) => x !== t)].slice(0, 8);
    setHistory(h);
    try { localStorage.setItem('aurora.searchHistory', JSON.stringify(h)); } catch { /* 防御 */ }
  };

  return (
    <div className="view">
      <div className="search-hero">
        <div className="search-hero-input">
          <span style={{ color: 'var(--text-tertiary)' }}>{I.search}</span>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && doSearch(query)}
            placeholder="用一句话描述要找的内容…"
            autoFocus
          />
          <button className="btn-text btn" style={{ minHeight: 36 }} onClick={() => doSearch(query)}>搜索</button>
        </div>
        <div className="search-hint">支持口语化查询，如「本周未完成的任务」「关联某项目的所有笔记」</div>

        {history.length > 0 && (
          <div className="search-history">
            {history.map((h) => (
              <button key={h} className="tag" style={{ cursor: 'pointer' }} onClick={() => { setQuery(h); doSearch(h); }}>
                {h}
              </button>
            ))}
          </div>
        )}
      </div>

      {results !== null && (
        <>
          <div className="result-section-title">笔记 · {results.length}</div>
          {results.length === 0 && <div className="empty-state"><div className="empty-text">没有找到相关内容</div></div>}
          <div className="note-list">
            {results.map((r) => (
              <div key={r.noteId} className="note-card" onClick={() => onOpen(r.noteId, r.title)}>
                <div className="note-card-body">
                  <div className="note-title">{r.title}</div>
                  <div className="note-snippet">{r.snippet}</div>
                  <div className="note-meta"><span className="search-hit-score">相关度 {(r.score * 100).toFixed(0)}%</span></div>
                </div>
              </div>
            ))}
          </div>

          <div className="result-section-title">任务 · 0</div>
          <div className="empty-state" style={{ padding: '12px 24px' }}><div className="empty-text">任务索引建设中</div></div>

          {/* V19 页面5: AI 生成 SQL 安全审计（可折叠） */}
          <button className="btn btn-text btn-block" onClick={() => setAuditOpen(!auditOpen)}>
            {auditOpen ? '收起' : '展开'}安全审计 ▾
          </button>
          {auditOpen && (
            <div className="card sql-audit">
              query = notes.search({JSON.stringify(query)}) →
              SELECT id, title, body FROM notes_idx WHERE body MATCH ? ORDER BY rank DESC LIMIT 20
            </div>
          )}
        </>
      )}
    </div>
  );
}
