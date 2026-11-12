export const K_TREE = 'aurora.tree';
/** 树域 — B-02 切片三: TreeNodeMeta/loadTree/saveTree 随 NotesView 迁出（纯搬移）。 */
export interface TreeNodeMeta {
  id: string;              // doc = noteId; book = 'b<ts>'（纯组织节点）
  kind: 'book' | 'doc';
  title: string;
  parentId: string | null; // book 为 null
}

export function loadTree(): TreeNodeMeta[] {
  try { return JSON.parse(localStorage.getItem(K_TREE) || 'null') || []; } catch { return []; }
}

export function saveTree(t: TreeNodeMeta[]) {
  try { localStorage.setItem(K_TREE, JSON.stringify(t)); } catch { /* 防御 */ }
}

/** E21 修: 初始树立即落盘（K_TREE）——否则 tab 切换 NodeTree 重挂→loadTree() 空→
 * 排序丢失（每次进树视图重置）。 */
export function initialTreePersisted(): TreeNodeMeta[] {
  const t = initialTree();
  saveTree(t);
  return t;
}

export function initialTree(): TreeNodeMeta[] {
  return [{ id: `b${Date.now()}`, kind: 'book', title: '我的笔记本', parentId: null }];
}
