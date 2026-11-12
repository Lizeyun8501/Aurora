/**
 * FavoritesView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, listNotesTitled, type Task } from '../components/primitives';

export function FavoritesView({ favs, onOpen }: { favs: string[]; onOpen: (id: string, title: string) => void }) {
  const notes = listNotesTitled().filter((n) => favs.includes(n.id));
  return (
    <div className="view">
      {notes.length === 0 ? (
        <EmptyState text="还没有收藏的笔记。在笔记编辑页菜单中点击「收藏」即可添加。" />
      ) : (
        <div className="note-list">
          {notes.map((n) => (
            <NoteCard key={n.id} note={n} onOpen={onOpen} />
          ))}
        </div>
      )}
    </div>
  );
}
