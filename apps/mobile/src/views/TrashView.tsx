/**
 * TrashView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, type Task, type TrashItem, loadList, saveList, K_TRASH } from '../components/primitives';

export function TrashView({ trash, onTrash, showToast }: {
  trash: TrashItem[];
  onTrash: React.Dispatch<React.SetStateAction<TrashItem[]>>;
  showToast: (m: string) => void;
}) {
  /** S2-B 真源（2026-11-01）: 渲染序取 platform.listTrashed()（bridge/core 或 mock aurora.trash）——
   * K_TRASH 演示态与 Dto 态并集渲染（旧数据兼容）;恢复/彻底删走真桥命令。 */
  const [dtoList, setDtoList] = useState(() => platform.listTrashed());
  useEffect(() => { setDtoList(platform.listTrashed()); }, [trash.length]);
  // 合并去重（K_TRASH 旧项 + Dto 项——noteId=id 语义同源）
  const dtoIds = new Set(dtoList.map((d) => d.noteId));
  const merged: Array<{ id: string; title: string; text: string; deletedAt: number }> = [
    ...trash.filter((t) => !dtoIds.has(t.id)).map((t) => ({ id: t.id, title: t.title, text: t.text, deletedAt: t.deletedAt })),
    ...dtoList.map((d) => ({ id: d.noteId, title: d.title, text: '', deletedAt: d.deletedAtMs })),
  ];
  merged.sort((a, b) => b.deletedAt - a.deletedAt);
  // S2-B 切真桥（2026-11-01）: restore 走 platform.restoreNote——保 id/时间戳/树位
  // （createNote 新 id=演示级旧路——丢收藏/最近/树位映射）;purge 同理。
  const restore = (t: { id: string; title: string; text: string; deletedAt: number }) => {
    const ok = platform.restoreNote(t.id);
    if (ok) {
      onTrash((prev) => { const n = prev.filter((x) => x.id !== t.id); saveList(K_TRASH, n); return n; });
      setDtoList(platform.listTrashed());
      showToast('已恢复（原位）');
    } else {
      showToast('恢复失败——已在回收站外');
    }
  };
  const purge = (t: { id: string; title: string; text: string; deletedAt: number }) => {
    const ok = platform.purgeNote(t.id);
    if (ok) {
      onTrash((prev) => { const n = prev.filter((x) => x.id !== t.id); saveList(K_TRASH, n); return n; });
      setDtoList(platform.listTrashed());
      showToast('已彻底删除');
    } else {
      showToast('删除失败——不在回收站');
    }
  };
  const clearAll = () => {
    platform.purgeExpired(0); // 0=全清（core 语义 days=0 即时过期）
    onTrash(() => { saveList(K_TRASH, []); return []; });
    setDtoList(platform.listTrashed());
    showToast('回收站已清空');
  };

  return (
    <div className="view">
      {merged.length === 0 ? (
        <EmptyState text="回收站是空的。删除的笔记会在这里保留，可随时恢复。" />
      ) : (
        <>
          <button className="btn btn-text btn-block" style={{ marginBottom: 8 }} onClick={clearAll}>
            清空回收站（{merged.length}）
          </button>
          <div className="note-list">
            {merged.map((t) => (
              <div key={t.id} className="note-card">
                <div className="note-card-body">
                  <div className="note-title">{t.title}</div>
                  <div className="note-snippet">{t.text.slice(0, 50) || '（无文本内容）'}</div>
                  <div className="note-meta">删除于 {relativeTime(new Date(t.deletedAt || Date.now()).toISOString())}</div>
                </div>
                <div className="trash-actions">
                  <button className="btn btn-secondary btn-sm" onClick={() => restore(t)}>
                    {I.restore} 恢复
                  </button>
                  <button className="btn btn-text btn-sm danger-text" onClick={() => purge(t)}>删除</button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
