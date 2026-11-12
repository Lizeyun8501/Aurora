/**
 * NotesView+NodeTree — B-02 切片三: 从 MobileApp 拆出（纯搬移零逻辑改动）。
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { type TreeNodeMeta, loadTree, saveTree, initialTreePersisted } from '../components/tree';
import { VivoIcon } from '../components/vivo';
import { BOOK_ICONS, DOC_ICONS, loadOrder, saveOrder, applyManualOrder, listNotesTitled, ALL_ICONS, relativeTime } from '../components/primitives';
import { platform, type NoteSummary } from '../adapters/androidPlatform';
import {
  I, NoteCard, EmptyState, type Task, getTitleOverlay, setTitleOverlay,
  getLevelOverlay, setLevelOverlay, noteSnippet, snippetCache, Switch,
  setIconDef, getIconDef,
} from '../components/primitives';

/** 文档树组件 — 展开折叠 + ＋新建子文档 + 孤儿笔记收入 + E21 图标更换。 */
export function NodeTree({ notes, onOpen, showToast, onSoftDelete }: {
  notes: NoteSummary[];
  onOpen: (id: string, title: string) => void;
  showToast: (m: string) => void;
  onSoftDelete: (id: string, title: string) => void;
}) {
  // E21: 图标选择器（sheet: 全集 16 格;选中落 overlay 持久,树行即时刷新）
  const [pick, setPick] = useState<{ id: string; kind: 'book' | 'doc' } | null>(null);
  const [, setIconTick] = useState(0);
  const onPickIcon = (id: string, kind: 'book' | 'doc') => setPick({ id, kind });
  const applyIcon = (iconId: string) => {
    if (!pick) return;
    setIconDef(pick.id, iconId);
    setIconTick((t) => t + 1); // 树行重读 overlay
    setPick(null);
    showToast('图标已更换');
  };
  const [tree, setTree] = useState<TreeNodeMeta[]>(() => {
    const t = loadTree();
    return t.length ? t : initialTreePersisted();
  });
  const persist = (t: TreeNodeMeta[]) => { saveTree(t); setTree(t); };
  // ===== E27（TA 23:16）: 树节点长按——上下文菜单/拖动移父/升降级 =====
  const [ctxId, setCtxId] = useState<string | null>(null);
  const [treeDrag, setTreeDrag] = useState<string | null>(null); // 拖动节点 id
  const [treeDragOver, setTreeDragOver] = useState<string | null>(null); // 落点行 id
  const press = useRef<ReturnType<typeof setTimeout> | null>(null);
  const ctxNode = tree.find((t) => t.id === ctxId) ?? null;

  const siblings = (pid: string | null) => tree.filter((t) => t.parentId === pid);
  /** 移节点到 target 行的父下、target 行后一位（拖动=移动+换父——层级变换一次覆盖）。 */
  const moveNode = (dragId: string, targetId: string) => {
    const drag = tree.find((t) => t.id === dragId);
    const target = tree.find((t) => t.id === targetId);
    if (!drag || !target || dragId === targetId) return;
    // 祖先环防: target 是 drag 的后代→拒（自吞树）
    let anc: string | null = target.parentId;
    while (anc) { if (anc === dragId) { showToast('不能移到自己的子节点下'); return; } anc = tree.find((t) => t.id === anc)?.parentId ?? null; }
    const newParent = target.kind === 'book' ? target.id : target.parentId;
    if (drag.parentId === newParent && treeDragOver === dragId) return; // 同位免动
    const rest = tree.filter((t) => t.id !== dragId);
    const ti = rest.findIndex((t) => t.id === targetId);
    const moved = { ...drag, parentId: newParent };
    rest.splice(ti + 1, 0, moved);
    persist(rest);
    showToast('已移动');
  };
  /** 升一级: 挂到原父的父下（原父同级后）。顶级/book 无法升。 */
  const levelUp = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n || !n.parentId) { showToast('已是顶级'); return; }
    const parent = tree.find((t) => t.id === n.parentId);
    if (!parent) { showToast('已是顶级'); return; }
    if (parent.kind === 'book' && !parent.parentId) {
      // 父是顶级笔记本→升为顶级 book? doc 不能成 book——语义边界: 移为顶级孤儿 doc 不合理,拒
      showToast('已是最上层（父为顶级笔记本）');
      return;
    }
    const rest = tree.filter((t) => t.id !== id);
    const pi = rest.findIndex((t) => t.id === parent.id);
    rest.splice(pi + 1, 0, { ...n, parentId: parent.parentId });
    persist(rest);
    showToast('已升一级');
  };
  /** 降一级: 挂到前一个同级节点下（成其末子）。无前同级→拒。 */
  const levelDown = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n) return;
    const sibs = siblings(n.parentId).filter((t) => t.id !== id);
    const idx = tree.findIndex((t) => t.id === id);
    const prevSame = [...tree.slice(0, idx)].reverse().find((t) => t.parentId === n.parentId && t.id !== id);
    if (!prevSame) { showToast('前面没有同级节点可挂靠'); return; }
    if (prevSame.kind !== 'book' && prevSame.id === id) return;
    const rest = tree.filter((t) => t.id !== id);
    rest.push({ ...n, parentId: prevSame.id });
    persist(rest);
    showToast(`已挂到「${prevSame.kind === 'book' ? prevSame.title : getTitleOverlay(prevSame.id) || prevSame.title}」下`);
  };
  /** E28b（TA 15:44）: 重命名——doc 双写（title overlay+树 meta）;book 树 meta。
   * overlay 优先显示（displayTitle 惯例）——core rename API 落地前的前端方案（同标题 overlay 注释）。 */
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameText, setRenameText] = useState('');
  const startRename = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n) return;
    setCtxId(null);
    setRenaming(id);
    setRenameText(displayTitle(n));
  };
  const commitRename = () => {
    if (!renaming) return;
    const name = renameText.trim();
    const n = tree.find((t) => t.id === renaming);
    if (!n || !name) { setRenaming(null); return; }
    if (n.kind === 'doc') setTitleOverlay(n.id, name);
    persist(tree.map((t) => (t.id === renaming ? { ...t, title: name } : t)));
    setRenaming(null);
    showToast('已重命名');
  };

  /** 删除: doc→软删（回收站）+摘树;book→摘树（子升孤儿）。 */
  const removeNode = (id: string) => {
    const n = tree.find((t) => t.id === id);
    if (!n) return;
    persist(tree.filter((t) => t.id !== id));
    if (n.kind === 'doc') {
      onSoftDelete(n.id, displayTitle(n));
      showToast('已移入回收站');
    } else {
      showToast('笔记本已移除（子笔记转入未入树区）');
    }
  };

  const [open, setOpen] = useState<Set<string>>(() => new Set(loadTree().filter((x) => x.kind === 'book').map((x) => x.id)));

  const toggle = (id: string) =>
    setOpen((prev) => { const n = new Set(prev); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const childrenOf = (pid: string | null) => tree.filter((t) => t.parentId === pid && !zombieIds.has(t.id));
  const firstBookId = tree.find((t) => t.kind === 'book')?.id ?? null;
  const displayTitle = (t: TreeNodeMeta) => (t.kind === 'doc' ? getTitleOverlay(t.id) || t.title : t.title);

  /** 新建子文档: 走现有 createNote（内容体系不动）, 树挂节点, 就地创建不跳转（对齐思源）。 */
  const createChild = (parentId: string) => {
    const id = platform.createNote('未命名文档');
    if (!id) { showToast('创建失败'); return; }
    persist([...tree, { id, kind: 'doc', title: '未命名文档', parentId }]);
    setOpen((prev) => new Set(prev).add(parentId));
    showToast('已创建子文档（点标题行改名）');
  };

  /** 平铺孤儿笔记收入第一个笔记本。 */
  const adopt = (n: NoteSummary) => {
    if (!firstBookId) return;
    persist([...tree, { id: n.note_id, kind: 'doc', title: n.title, parentId: firstBookId }]);
    showToast(`「${n.title}」已收入笔记本`);
  };

  // E28a: 僵尸 doc 过滤——platform.listNotes() 实查（props notes state 滞后一拍会误杀
  // createChild 新建节点——dbg 实证;实查 mock=localStorage 真机=桥,幂等且实时）
  let liveIds: Set<string>;
  try { liveIds = new Set(platform.listNotes().map((n) => n.note_id)); } catch { liveIds = new Set(); }
  const zombieIds = new Set(tree.filter((t) => t.kind === 'doc' && !liveIds.has(t.id)).map((t) => t.id));
  const treeDocIds = new Set(tree.filter((t) => t.kind === 'doc').map((t) => t.id));
  const orphans = notes.filter((n) => !treeDocIds.has(n.note_id));

  const row = (t: TreeNodeMeta, depth: number): React.ReactNode => {
    const kids = childrenOf(t.id);
    const hasKids = kids.length > 0;
    const isOpen = open.has(t.id);
    return (
      <React.Fragment key={t.id}>
        <div
          className={`nt-row ${t.kind === 'book' ? 'nt-book' : ''} ${treeDrag === t.id ? 'dragging' : ''} ${treeDragOver === t.id && treeDrag && treeDrag !== t.id ? 'drag-over' : ''}`}
          style={{ paddingLeft: 8 + depth * 18 }}
          data-node-id={t.id}
          onTouchStart={(e) => {
            if (treeDrag) return; // 拖动态不加新长按
            if (press.current) clearTimeout(press.current);
            press.current = setTimeout(() => { setCtxId(t.id); }, 400);
          }}
          onTouchMove={() => {
            if (press.current) { clearTimeout(press.current); press.current = null; }
            if (treeDrag && treeDrag !== t.id) setTreeDragOver(t.id);
          }}
          onTouchEnd={(e) => {
            if (press.current) { clearTimeout(press.current); press.current = null; }
            // E27: 长按刚弹菜单——阻合成 click 穿 backdrop 秒关（touchend preventDefault 阻 click 合成）
            if (ctxId) { e.preventDefault(); return; }
            if (treeDrag) {
              if (treeDragOver && treeDragOver !== treeDrag) moveNode(treeDrag, treeDragOver);
              setTreeDrag(null); setTreeDragOver(null);
            }
          }}
        >
          {hasKids || t.kind === 'book' ? (
            <button className="nt-arrow" onClick={() => toggle(t.id)} aria-label={isOpen ? '折叠' : '展开'}>{isOpen ? '▾' : '▸'}</button>
          ) : <span className="nt-arrow nt-arrow-ph" />}
          {/* E21: VIVO 图标块——点击开选择器换图标（overlay 持久） */}
          <button className="nt-icon-btn" aria-label="更换图标" title="更换图标"
            onClick={() => onPickIcon?.(t.id, t.kind)}><VivoIcon def={getIconDef(t.kind, t.id)} /></button>
          <button className="nt-title" onClick={() => t.kind === 'doc' && onOpen(t.id, displayTitle(t))}>
            {displayTitle(t)}
            {t.kind === 'doc' && hasKids && <span className="nt-count">{kids.length}</span>}
          </button>
          <button className="nt-add" onClick={() => createChild(t.id)} aria-label="新建子文档">＋</button>
        </div>
        {isOpen && kids.map((k) => row(k, depth + 1))}
      </React.Fragment>
    );
  };

  return (
    <div>
      {treeDrag && <div className="tree-drag-tip">拖动中——移到目标行松手（落点行即新位置/新层级）</div>}
      {tree.filter((t) => t.kind === 'book').map((b) => row(b, 0))}
      <button className="btn btn-text" style={{ marginTop: 4 }} onClick={() => {
        const id = `b${Date.now()}`;
        const name = `笔记本 ${tree.filter((t) => t.kind === 'book').length + 1}`;
        persist([...tree, { id, kind: 'book', title: name, parentId: null }]);
        showToast(`已创建「${name}」`);
      }}>＋ 新建笔记本</button>

      {orphans.length > 0 && (
        <>
          <div className="nt-orphan-title">未入树笔记（{orphans.length}）— 点「收入」挂到笔记本</div>
          {orphans.map((n) => (
            <div key={n.note_id} className="nt-row" style={{ paddingLeft: 8 }}>
              <span className="nt-arrow nt-arrow-ph" />
              <button className="nt-icon-btn" aria-label="更换图标" title="更换图标"
                onClick={() => onPickIcon?.(n.note_id, 'doc')}><VivoIcon def={getIconDef('doc', n.note_id)} /></button>
              <button className="nt-title" onClick={() => onOpen(n.note_id, n.title)}>{n.title}</button>
              <button className="nt-add" style={{ fontSize: 12 }} onClick={() => adopt(n)}>收入</button>
            </div>
          ))}
        </>
      )}

      {/* E27（TA 23:16）: 树节点长按菜单——删除/换图标/升降级/拖动排序 */}
      {ctxNode && (
        <div className="ctx-sheet-backdrop" onClick={() => setCtxId(null)}>
          <div className="ctx-sheet" role="menu" aria-label="节点操作" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">
              {ctxNode.kind === 'book' ? '📁' : '📄'} {displayTitle(ctxNode)}
            </div>
            <button className="ctx-item danger" onClick={() => { setCtxId(null); removeNode(ctxNode.id); }}>🗑 删除{ctxNode.kind === 'book' ? '笔记本（子笔记转未入树区）' : '（移入回收站）'}</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); onPickIcon(ctxNode.id, ctxNode.kind); }}>🎨 更换图标</button>
            <button className="ctx-item" onClick={() => startRename(ctxNode.id)}>✏️ 重命名</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); levelUp(ctxNode.id); }}>↖ 升一级</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); levelDown(ctxNode.id); }}>↘ 降一级</button>
            <button className="ctx-item" onClick={() => { setCtxId(null); setTreeDrag(ctxNode.id); showToast('拖到目标行松手（换位置/换层级）'); }}>⇅ 拖动排序</button>
            <button className="ctx-item ctx-cancel" onClick={() => setCtxId(null)}>取消</button>
          </div>
        </div>
      )}

      {/* E28b: 重命名 sheet——内联输入+确认 */}
      {renaming && (
        <div className="ctx-sheet-backdrop" onClick={() => setRenaming(null)}>
          <div className="ctx-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">重命名</div>
            <input
              className="rename-input" autoFocus value={renameText}
              onChange={(e) => setRenameText(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') commitRename(); }}
              aria-label="新名称" placeholder="输入新名称"
            />
            <button className="ctx-item" onClick={commitRename}>确认</button>
            <button className="ctx-item ctx-cancel" onClick={() => setRenaming(null)}>取消</button>
          </div>
        </div>
      )}

      {/* E21 图标选择器 sheet: 16 格全集（book/doc 8+8）;点击即换+toast */}
      {pick && (
        <>
          <div className="sheet-mask" onClick={() => setPick(null)} />
          <div className="sheet" style={{ paddingBottom: 14 }}>
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">更换图标</span>
              <button className="icon-btn" onClick={() => setPick(null)} aria-label="关闭">{I.x}</button>
            </div>
            <div style={{ padding: '6px 12px 0' }}>
              <div style={{ fontSize: 12, color: 'var(--text-tertiary)', marginBottom: 8 }}>{pick.kind === 'book' ? '笔记本' : '文档'}图标</div>
              <div className="vivo-icon-grid">
                {(pick.kind === 'book' ? BOOK_ICONS : DOC_ICONS).map((ic) => (
                  <button key={ic.id} className="vivo-icon-cell" title={ic.label} aria-label={ic.label}
                    onClick={() => applyIcon(ic.id)}>
                    <VivoIcon def={ic} size={34} />
                  </button>
                ))}
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

export function NotesView({ onOpen, showToast, onNewNote, onSoftDelete, favs, onToggleFav, setNoteEncryption }: {
  onOpen: (id: string, title: string) => void;
  showToast: (t: string) => void;
  onNewNote: () => void;
  onSoftDelete: (id: string, title: string) => void;
  favs: string[];
  onToggleFav: (id: string) => void;
  setNoteEncryption: (id: string, level: 'none' | 'aes256gcm') => boolean;
}) {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [query, setQuery] = useState('');
  const [swipingId, setSwipingId] = useState<string | null>(null);
  const [mode, setMode] = useState<'tree' | 'list'>('tree');
  /** v4 长按拖拽排序（VIVO 原子笔记对标）: 拖起 id / 落点索引 / 长按定时器 / 手动序 overlay。 */
  const [dragId, setDragId] = useState<string | null>(null);
  // E27（TA 23:16）: 长按上下文菜单——删除/换图标/升降级/拖动排序
  const [ctxId, setCtxId] = useState<string | null>(null);
  const [ctxIconPick, setCtxIconPick] = useState(false); // 换图标宫格展开态
  const [uiVer, setUiVer] = useState(0); // overlay（level/icon）变更 bump——重渲染
  const [dragOverIdx, setDragOverIdx] = useState<number | null>(null);
  const pressTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [order, setOrder] = useState<string[]>(() => loadOrder());

  const refresh = useCallback(() => setNotes(listNotesTitled()), []);
  useEffect(refresh, [refresh]);

  /** 左滑删除（V19 移动端页面1 触屏交互）
   * ⚠️ hook 必须在 mode 条件 early return 之前——树形轮曾跳过此声明，
   * 切列表时 hooks 数量 3→4 触发 React #310 渲染崩溃（v24 真机首报）。 */
  const cardTouch = useRef<{ x: number; y: number } | null>(null);

  /** 视图段控 — 树形（思源式笔记本/文档树）/ 列表（原平铺）。 */
  const modeSeg = (
    <div style={{ display: 'flex', gap: 6, marginBottom: 12 }}>
      <button className={`nt-seg ${mode === 'tree' ? 'on' : ''}`} onClick={() => setMode('tree')}>树形</button>
      <button className={`nt-seg ${mode === 'list' ? 'on' : ''}`} onClick={() => setMode('list')}>列表</button>
    </div>
  );

  if (mode === 'tree') {
    return (
      <div className="view">
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
          <span style={{ fontSize: 20, fontWeight: 700 }}>文档</span>
          {modeSeg}
        </div>
        <NodeTree notes={notes} onOpen={onOpen} showToast={showToast} onSoftDelete={onSoftDelete} />
      </div>
    );
  }

  const ordered = applyManualOrder(notes, order); // v4 手动序（搜索态保持过滤后原序——手动序仅主导无搜索态）
  const shown = query.trim()
    ? notes.filter((n) => n.title.includes(query.trim()))
    : ordered;
  // E27: ctxNote 在 shown 后求值（声明序）
  const ctxNote = ctxId ? shown.find((n) => n.id === ctxId) ?? notes.find((n) => n.id === ctxId) : null;

  const onMove = (e: React.TouchEvent, id: string) => {
    if (!cardTouch.current) return;
    const dx = e.touches[0].clientX - cardTouch.current.x;
    if (dx < -64) setSwipingId(id); // 左滑露删除/加密（既有语义）
    if (dx > 64) setSwipingId(`fav:${id}`); // 右滑露收藏（VIVO 原子笔记·右滑设重要度对标）
  };

  /** v4 长按拖拽: touchstart 起 350ms 定时器（未到期 touchmove=滑动取消）;拖起后 touchmove 换落点索引。 */
  const onCardTouchStart = (e: React.TouchEvent, id: string) => {
    cardTouch.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };
    if (pressTimer.current) clearTimeout(pressTimer.current);
    // E27: 长按=弹上下文菜单（拖动排序经菜单入口——VIVO 惯例;旧「长按直接拖」废止）
    pressTimer.current = setTimeout(() => { setCtxId(id); setCtxIconPick(false); }, 400);
  };
  const onCardTouchMove = (e: React.TouchEvent, id: string, idx: number) => {
    if (!cardTouch.current) return;
    const dx = e.touches[0].clientX - cardTouch.current.x;
    const dy = e.touches[0].clientY - cardTouch.current.y;
    if (dragId === null) {
      // 非拖动态: 位移>10 即取消长按判定（滑动 vs 长按二分）
      if (Math.abs(dx) > 10 || Math.abs(dy) > 10) {
        if (pressTimer.current) { clearTimeout(pressTimer.current); pressTimer.current = null; }
        if (dx < -64) setSwipingId(id);
        if (dx > 64) setSwipingId(`fav:${id}`);
      }
      return;
    }
    // 拖动态: 记落点
    if (dragId !== id) setDragOverIdx(idx);
  };
  /** E27 长按菜单操作。 */
  const ctxStartDrag = (id: string) => { setCtxId(null); setCtxIconPick(false); setDragId(id); showToast('拖动到目标位置后松手'); };
  const ctxLevel = (id: string, delta: number) => {
    const cur = getLevelOverlay(id);
    const next = Math.max(0, Math.min(3, cur + delta));
    if (next === cur) { showToast(delta > 0 ? '已是最深层级' : '已是顶级'); return; }
    setLevelOverlay(id, next);
    setUiVer((v) => v + 1);
    showToast(`层级: ${next === 0 ? '顶级' : `L${next}`}`);
  };
  const ctxDelete = (id: string, title: string) => {
    setCtxId(null); setCtxIconPick(false);
    onSoftDelete(id, title);
    showToast('已移入回收站');
  };
  const ctxSetIcon = (id: string, iconId: string) => {
    setIconDef(id, iconId);
    setUiVer((v) => v + 1);
    setCtxIconPick(false);
    setCtxId(null);
    showToast('图标已更换');
  };
  const onCardTouchEnd = () => {
    if (pressTimer.current) { clearTimeout(pressTimer.current); pressTimer.current = null; }
    cardTouch.current = null;
    if (dragId !== null) {
      // 落定: 以 dragOverIdx 为目标位重建序（未记录落点=原位取消）
      if (dragOverIdx !== null && dragOverIdx >= 0) {
        const ids = applyManualOrder(notes, order).map((n) => n.id);
        const arr = ids.filter((x) => x !== dragId);
        const to = Math.min(dragOverIdx, arr.length);
        arr.splice(to, 0, dragId);
        setOrder(arr);
        saveOrder(arr);
        showToast('顺序已保存');
      }
      setDragId(null);
      setDragOverIdx(null);
    }
  };

  return (
    <div className="view">
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
        <span style={{ fontSize: 20, fontWeight: 700 }}>知识库</span>
        {modeSeg}
      </div>
      <div className="search-hero-input" style={{ marginBottom: 12 }}>
        <span style={{ color: 'var(--text-tertiary)' }}>{I.search}</span>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="筛选笔记…"
        />
      </div>

      {shown.length === 0 ? (
        <div className="empty-state">
          <div className="empty-text">
            {query ? '没有匹配的笔记' : '新建笔记 / 从收件箱导入素材'}
            {!query && (
              <><br /><button className="btn btn-text" onClick={onNewNote}>+ 新建笔记</button></>
            )}
          </div>
        </div>
      ) : (
        <div className="note-list">
          {shown.map((n, idx) => (
            <div
              key={n.id}
              className={`note-card ${swipingId === n.id || swipingId === `fav:${n.id}` ? 'swiped' : ''} ${swipingId === `fav:${n.id}` ? 'swiped-fav' : ''} ${dragId === n.id ? 'dragging' : ''} ${dragOverIdx === idx && dragId !== null && dragId !== n.id ? 'drag-over' : ''}`}
              onTouchStart={(e) => onCardTouchStart(e, n.id)}
              onTouchMove={(e) => onCardTouchMove(e, n.id, idx)}
              onTouchEnd={onCardTouchEnd}
              onClick={() => {
                // 滑动态: 点 body=收菜单（原子笔记语义）;非滑动态=进编辑器。
                if (swipingId === n.id || swipingId === `fav:${n.id}`) { setSwipingId(null); return; }
                onOpen(n.id, n.title);
              }}
            >
              <div className="note-card-body" style={{ marginLeft: getLevelOverlay(n.id) * 24 }} data-ui-ver={uiVer}>
                <div className="note-title">
                  <VivoIcon def={getIconDef('doc', n.id)} size={20} />
                  {' '}
                  {n.encryption === 'aes256gcm' && (
                    <span style={{ marginRight: 4 }} aria-label="已加密" title="加密笔记 — 锁定态搜索/导出不可见">🔒</span>
                  )}
                  {n.title}
                </div>
                <div className="note-snippet">{noteSnippet(n.id) || '空笔记'}</div>
                <div className="note-meta">
                  <span>{relativeTime(n.updatedAt)}</span>
                  <span className="tag">Inbox</span>
                  {n.encryption === 'aes256gcm' && <span className="tag">加密</span>}
                </div>
              </div>
              <button
                className={`fav-btn ${favs.includes(n.id) ? 'on' : ''}`}
                onClick={(e) => { e.stopPropagation(); onToggleFav(n.id); }}
                aria-label="收藏"
              >{I.star}</button>
              <span className="note-card-chevron">{I.chev}</span>
              {swipingId === n.id && (
                <>
                  <button
                    className="swipe-delete"
                    onClick={(e) => {
                      e.stopPropagation();
                      onSoftDelete(n.id, n.title);
                      setSwipingId(null);
                      refresh();
                      showToast('已移入回收站');
                    }}
                  >删除</button>
                  <button
                    className="swipe-delete"
                    style={{ background: 'var(--text-tertiary)' }}
                    onClick={(e) => {
                      e.stopPropagation();
                      const to = n.encryption === 'aes256gcm' ? 'none' : 'aes256gcm';
                      const okk = setNoteEncryption(n.id, to);
                      setSwipingId(null);
                      refresh();
                      showToast(
                        okk
                          ? (to === 'aes256gcm' ? '🔒 已加密锁定' : '已解锁')
                          : '锁定失败（不支持或已锁定）',
                      );
                    }}
                  >{n.encryption === 'aes256gcm' ? '解锁' : '加密'}</button>
                </>
              )}
              {swipingId === `fav:${n.id}` && (
                <button
                  className="swipe-fav"
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleFav(n.id);
                    setSwipingId(null);
                    showToast(favs.includes(n.id) ? '已取消收藏' : '★ 已收藏');
                  }}
                  aria-label={favs.includes(n.id) ? '取消收藏' : '收藏'}
                >{favs.includes(n.id) ? '取消收藏' : '★ 收藏'}</button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* E27（TA 23:16）: 长按上下文菜单——底部 sheet: 删除/换图标/升降级/拖动排序 */}
      {ctxNote && (
        <div className="ctx-sheet-backdrop" onClick={() => { setCtxId(null); setCtxIconPick(false); }}>
          <div className="ctx-sheet" role="menu" aria-label="笔记操作" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">{ctxNote.title || '未命名'}</div>
            {!ctxIconPick ? (
              <>
                <button className="ctx-item" onClick={() => ctxDelete(ctxNote.id, ctxNote.title)}>🗑 删除（移入回收站）</button>
                <button className="ctx-item" onClick={() => setCtxIconPick(true)}>🎨 更换图标</button>
                <button className="ctx-item" onClick={() => ctxLevel(ctxNote.id, -1)}>↖ 升一级（减少缩进）</button>
                <button className="ctx-item" onClick={() => ctxLevel(ctxNote.id, 1)}>↘ 降一级（增加缩进）</button>
                <button className="ctx-item" onClick={() => ctxStartDrag(ctxNote.id)}>⇅ 拖动排序</button>
                <button className="ctx-item ctx-cancel" onClick={() => { setCtxId(null); setCtxIconPick(false); }}>取消</button>
              </>
            ) : (
              <>
                <div className="ctx-sheet-title">选择图标</div>
                <div className="vivo-icon-grid">
                  {ALL_ICONS.map((ic) => (
                    <button key={ic.id} className="vivo-icon-cell" title={ic.label} aria-label={ic.label}
                      onClick={() => ctxSetIcon(ctxNote.id, ic.id)}>
                      <VivoIcon def={ic} size={26} />
                    </button>
                  ))}
                </div>
                <button className="ctx-item ctx-cancel" onClick={() => setCtxIconPick(false)}>返回</button>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
