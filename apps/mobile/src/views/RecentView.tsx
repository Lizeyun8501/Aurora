/**
 * RecentView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import {I, NoteCard, relativeTime, type Task, type RecentItem, listNotesTitled} from '../components/primitives';
import type { SearchResult, CalEvent } from '../adapters/androidPlatform';
import { EmptyState } from '@aurora/ui-components';

export function RecentView({ recents, onOpen }: { recents: RecentItem[]; onOpen: (id: string, title: string) => void }) {
  return (
    <div className="view">
      {recents.length === 0 ? (
        <EmptyState illustration="search" title="最近浏览记录将出现在这里。" />
      ) : (
        <div className="note-list">
          {recents.map((r) => (
            <div key={r.id} className="note-card" role="button" tabIndex={0}
              aria-label={`打开笔记 ${r.title}`}
              onClick={() => onOpen(r.id, r.title)}
              onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onOpen(r.id, r.title); } }}>
              <div className="note-card-body">
                <div className="note-title">{r.title}</div>
                <div className="note-meta">{relativeTime(new Date(r.at).toISOString())}浏览</div>
              </div>
              <span className="note-card-chevron" aria-hidden="true">›</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
