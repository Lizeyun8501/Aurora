/**
 * Android WebView 平台适配器 — V19 §36.3 正式方案。
 *
 * 数据链路: React → window.AndroidBridge (@JavascriptInterface)
 *         → Java UniffiAppCore → JNI → Rust aurora-mobile-ffi → Core
 *
 * 当运行在浏览器（开发预览）时自动降级为 localStorage mock。
 */

export interface NoteSummary {
  id: string;
  /** JNI 侧同义别名（MobileApp 目录树消费 note_id 字段名） */
  note_id: string;
  title: string;
  updatedAt: string;

  /** DK-07: 笔记加密级别（"none" | "aes256gcm"） */
  encryption: string;}

export interface SearchResult {
  noteId: string;
  title: string;
  snippet: string;
  score: number;
}

interface AndroidBridge {
  init(dataDir: string): string;
  listNotes(): string;
  createNote(title: string): string;
  getNoteContent(noteId: string): string;
  /** V26 M2/DK-06: 周回顾汇总 JSON — (estimate/actual/deviation/tasks) */
  todayFocusSummary(): string;
  /** DK-07 S4: 笔记加密切换 — false = 非法级别/笔记不存在/降级 */
  setNoteEncryption(noteId: string, level: string): boolean;
  /** DK-08 §7.3: 仅 Wi-Fi 同步开关（未初始化/异常 = false 不限制） */
  getWifiOnly(): boolean;
  setWifiOnly(on: boolean): void;
  saveNoteContent(noteId: string, content: string): number;
  deleteNote(noteId: string): number;
  searchNotes(query: string): string;
  isFallback(): boolean;
  // ProseMirror + Loro 编辑器（V19 §35 DEV-009）
  getNoteSnapshot(noteId: string): string;
  saveNoteSnapshot(noteId: string, snapshotBase64: string): boolean;
  listSnapshots(noteId: string): string;
  getSnapshotContent(noteId: string, version: number): string | null;
  // P2P 同步（V19 §31 DEV-005 — iroh QUIC + NAT 穿透）
  startSyncEngine(): string;
  getSyncLocalAddr(): string;
  syncNote(peerAddrJson: string, noteId: string): string;
  startAcceptSync(noteId: string): boolean;
  closeSyncEngine(): void;
  // DK-49：移动端同步数据面七函数（JSON 透传，与桌面 DTO 同构）
  syncStatus(): string;
  queueList(): string;
  queueDrain(): string;
  conflictList(): string;
  conflictResolve(artifactId: string): boolean;
  conflictResolveSemantic(conflictId: string, resolution: string): boolean;
}

/** P2P 同步结果报告。 */
export interface P2pSyncReport {
  success: boolean;
  sentBytes: number;
  receivedBytes: number;
  remotePeer: string;
  error: string;
}

// ---------------------------------------------------------------------------
// DK-49：同步数据面 DTO（与桌面 SyncStatusDto/QueueItemDto/DrainReportDto/
// ConflictDto 逐字段同构 — DK-50 三组件移植的契约面）
// ---------------------------------------------------------------------------

/** 同步状态聚合（gate 三态：Allow / DeferUntilUnmetered / DeferUntilOnline）。 */
export interface SyncStatus {
  gate: string;
  wifi_only: boolean;
  queue_len: number;
  pending_artifacts: number;
  pending_semantic: number;
}

/** 离线队列条目。 */
export interface SyncQueueItem {
  id: string;
  doc_id: string;
  priority: number;
  attempts: number;
  created_at: string;
}

/** 补发结果报告（failed_docs 不可丢）。 */
export interface SyncDrainReport {
  synced_docs: number;
  failed_docs: string[];
  acked_items: number;
}

/** 待处理冲突行（kind: artifact=真冲突副本 / semantic=语义冲突）。 */
export interface SyncConflictItem {
  kind: 'artifact' | 'semantic';
  id: string;
  doc_id: string;
  summary: string;
  created_at: string;
}

declare global {
  interface Window {
    AndroidBridge?: AndroidBridge;
  }
}

function bridge(): AndroidBridge | undefined {
  return typeof window !== 'undefined' ? window.AndroidBridge : undefined;
}

/** 是否运行在 Android WebView 内。 */
export function isAndroid(): boolean {
  return bridge() !== undefined;
}

// ---------------------------------------------------------------------------
// localStorage mock — 浏览器开发预览用
// ---------------------------------------------------------------------------

interface StoredNote {
  id: string;
  title: string;
  content: string;
  updatedAt: string;
}

// DK-05M：无桥（file://）环境快照 mock —— 空 LoroDoc snapshot 常量（node loro-crdt export 生成）
const EMPTY_SNAPSHOT_B64 =
  'bG9ybwAAAAAAAAAAAAAAAFMuDPwAAy8AAABMT1JPAAAAAgB2dgAAAAEAAgDnYslsAQAAAAUAAAACAGZyAAIAdnbb2MnIFgAAAAAAAAAAAAAA';
const snapKey = (id: string) => `aurora.snap.${id}`;
const mockGetSnapshot = (id: string): string | null => {
  try {
    return localStorage.getItem(snapKey(id));
  } catch {
    return null;
  }
};
const mockSaveSnapshot = (id: string, b64: string): boolean => {
  try {
    localStorage.setItem(snapKey(id), b64);
    return true;
  } catch {
    return false;
  }
};

function mockNotes(): StoredNote[] {
  try {
    return JSON.parse(localStorage.getItem('aurora.notes') ?? '[]') as StoredNote[];
  } catch {
    return [];
  }
}

function mockSave(notes: StoredNote[]): void {
  localStorage.setItem('aurora.notes', JSON.stringify(notes));
}

// ---------------------------------------------------------------------------
// DK-49：同步数据面 mock 内存态 + 调用录制（DoD3 — 前端开发不依赖真机；
// 录制数组供测试断言「回放」，setMockSyncState 供场景注入）
// ---------------------------------------------------------------------------

interface MockSyncState {
  wifiOnly: boolean;
  queue: SyncQueueItem[];
  conflicts: SyncConflictItem[];
}

const mockSyncState: MockSyncState = { wifiOnly: false, queue: [], conflicts: [] };

/** mock 模式下 7 调用录制（method + args — 回放断言用）。 */
export const mockSyncCalls: Array<{ method: string; args?: unknown }> = [];

/** 测试/开发注入 mock 场景（覆盖内存态 — 不影响真机 bridge 路径）。 */
export function setMockSyncState(state: Partial<MockSyncState>): void {
  Object.assign(mockSyncState, state);
}

/** 清空录制并重置内存态（测试隔离用）。 */
export function resetMockSync(): void {
  mockSyncCalls.length = 0;
  mockSyncState.wifiOnly = false;
  mockSyncState.queue = [];
  mockSyncState.conflicts = [];
}

// ---------------------------------------------------------------------------
// 平台 API — 与 Rust CoreAPI 对齐 (V19 §36.3)
// ---------------------------------------------------------------------------

export const platform = {
  init(dataDir: string): boolean {
    const b = bridge();
    if (b) return b.init(dataDir) === 'ok';
    return true; // mock 模式永远成功
  },

  isFallback(): boolean {
    return bridge()?.isFallback() ?? false;
  },

  /** DK-07: 设置笔记加密级别（fail-closed：非法级别/降级模式返回 false） */
  setNoteEncryption(noteId: string, level: 'none' | 'aes256gcm'): boolean {
    const b = bridge();
    if (!b) return false;
    try {
      return b.setNoteEncryption(noteId, level) === true;
    } catch {
      return false;
    }
  },

  listNotes(): NoteSummary[] {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.listNotes()) as NoteSummary[];
      } catch {
        return [];
      }
    }
    return mockNotes().map(({ id, title, updatedAt }) => ({ id, note_id: id, title, updatedAt, encryption: 'none' }));
  },

  createNote(title: string): string | null {
    const b = bridge();
    if (b) {
      const id = b.createNote(title);
      return id || null;
    }
    const notes = mockNotes();
    const note: StoredNote = {
      id: `note-${Date.now()}`,
      title,
      content: '',
      updatedAt: new Date().toISOString(),
    };
    notes.unshift(note);
    mockSave(notes);
    mockSaveSnapshot(note.id, EMPTY_SNAPSHOT_B64); // DK-05M：新建笔记即建空快照（rich 模式使能）
    return note.id;
  },

  getNoteContent(noteId: string): string {
    const b = bridge();
    if (b) return b.getNoteContent(noteId) ?? '';
    return mockNotes().find((n) => n.id === noteId)?.content ?? '';
  },
  /** V26 M2/DK-06: 周回顾汇总（fallback 返回 null — UI 降级本地计算） */
  todayFocusSummary(): { estimate: number; actual: number; deviation: number | null; tasks: number } | null {
    const b = bridge();
    if (!b) return null;
    try {
      const o = JSON.parse(b.todayFocusSummary());
      if (typeof o.estimate === 'number' && typeof o.actual === 'number') return o;
      return null;
    } catch {
      return null;
    }
  },

  saveNoteContent(noteId: string, content: string): boolean {
    const b = bridge();
    if (b) return b.saveNoteContent(noteId, content) === 0;
    const notes = mockNotes();
    const n = notes.find((x) => x.id === noteId);
    if (n) {
      n.content = content;
      n.updatedAt = new Date().toISOString();
      mockSave(notes);
      return true;
    }
    return false;
  },

  deleteNote(noteId: string): boolean {
    const b = bridge();
    if (b) return b.deleteNote(noteId) === 0;
    mockSave(mockNotes().filter((n) => n.id !== noteId));
    return true;
  },

  searchNotes(query: string): SearchResult[] {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.searchNotes(query)) as SearchResult[];
      } catch {
        return [];
      }
    }
    const q = query.toLowerCase();
    return mockNotes()
      .filter((n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q))
      .map((n) => ({
        noteId: n.id,
        title: n.title,
        snippet: n.content.slice(0, 80),
        score: 1,
      }));
  },

  // -------------------------------------------------------------------------
  // DK-08 §7.3 仅 Wi-Fi 同步 — mock 模式恒 false（不限制）+ 无操作
  // -------------------------------------------------------------------------

  getWifiOnly(): boolean {
    return bridge()?.getWifiOnly() ?? false;
  },

  setWifiOnly(on: boolean): void {
    try {
      bridge()?.setWifiOnly(on);
    } catch {
      // 浏览器 mock / 桥异常：静默无操作
    }
  },

  // -------------------------------------------------------------------------
  // DK-49：同步数据面七函数（bridge 优先 + mock 内存态降级 — 录制模式）
  // -------------------------------------------------------------------------

  /** 函数 1：同步状态聚合。桥异常/mock = 内存态聚合（gate=DeferUntilOnline）。 */
  syncStatus(): SyncStatus {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.syncStatus()) as SyncStatus;
      } catch {
        /* fallthrough to mock */
      }
    }
    mockSyncCalls.push({ method: 'syncStatus' });
    return {
      gate: 'DeferUntilOnline',
      wifi_only: mockSyncState.wifiOnly,
      queue_len: mockSyncState.queue.length,
      pending_artifacts: mockSyncState.conflicts.filter((c) => c.kind === 'artifact').length,
      pending_semantic: mockSyncState.conflicts.filter((c) => c.kind === 'semantic').length,
    };
  },

  /** 函数 2：离线队列明细。桥异常/mock = 内存队列。 */
  queueList(): SyncQueueItem[] {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.queueList()) as SyncQueueItem[];
      } catch {
        return [];
      }
    }
    mockSyncCalls.push({ method: 'queueList' });
    return [...mockSyncState.queue];
  },

  /** 函数 3：手动补发。桥错误返回 {error}；mock = 消耗内存队列出报告。 */
  queueDrain(): SyncDrainReport | { error: string } {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.queueDrain()) as SyncDrainReport | { error: string };
      } catch {
        return { error: 'queue drain failed' };
      }
    }
    mockSyncCalls.push({ method: 'queueDrain' });
    const report: SyncDrainReport = {
      synced_docs: mockSyncState.queue.length,
      failed_docs: [],
      acked_items: mockSyncState.queue.length,
    };
    mockSyncState.queue = [];
    return report;
  },

  /** 函数 4：待处理冲突合并视图。桥异常/mock = 内存冲突表。 */
  conflictList(): SyncConflictItem[] {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.conflictList()) as SyncConflictItem[];
      } catch {
        return [];
      }
    }
    mockSyncCalls.push({ method: 'conflictList' });
    return [...mockSyncState.conflicts];
  },

  /** 函数 5：真冲突副本闭环。桥失败/mock 未命中返回 false。 */
  conflictResolve(artifactId: string): boolean {
    const b = bridge();
    if (b) {
      try {
        return b.conflictResolve(artifactId) === true;
      } catch {
        return false;
      }
    }
    mockSyncCalls.push({ method: 'conflictResolve', args: artifactId });
    const before = mockSyncState.conflicts.length;
    mockSyncState.conflicts = mockSyncState.conflicts.filter(
      (c) => !(c.kind === 'artifact' && c.id === artifactId),
    );
    return mockSyncState.conflicts.length < before;
  },

  /** 函数 6：语义冲突解决。桥失败/mock 未命中返回 false。 */
  conflictResolveSemantic(conflictId: string, resolution: string): boolean {
    const b = bridge();
    if (b) {
      try {
        return b.conflictResolveSemantic(conflictId, resolution) === true;
      } catch {
        return false;
      }
    }
    mockSyncCalls.push({ method: 'conflictResolveSemantic', args: { conflictId, resolution } });
    const before = mockSyncState.conflicts.length;
    mockSyncState.conflicts = mockSyncState.conflicts.filter(
      (c) => !(c.kind === 'semantic' && c.id === conflictId),
    );
    return mockSyncState.conflicts.length < before;
  },

  // -------------------------------------------------------------------------
  // ProseMirror + Loro 编辑器（V19 §35 DEV-009）
  // -------------------------------------------------------------------------

  /** 获取笔记 Loro 快照（base64）。无桥/失败返回 null。 */
  getNoteSnapshot(noteId: string): string | null {
    const b = bridge()?.getNoteSnapshot(noteId);
    if (b !== undefined && b !== null) return b;
    return mockGetSnapshot(noteId); // DK-05M：无桥快照 mock（file:// rich 模式）
  },

  /** 保存 Loro 快照（base64，CRDT 合并语义）。 */
  saveNoteSnapshot(noteId: string, snapshotBase64: string): boolean {
    const b = bridge();
    if (b) return b.saveNoteSnapshot(noteId, snapshotBase64);
    return mockSaveSnapshot(noteId, snapshotBase64); // DK-05M：无桥快照 mock
  },

  listSnapshots(noteId: string): string {
    return bridge()?.listSnapshots(noteId) ?? '[]';
  },

  getSnapshotContent(noteId: string, version: number): string | null {
    return bridge()?.getSnapshotContent(noteId, version) ?? null;
  },

  // -------------------------------------------------------------------------
  // P2P 同步（V19 §31 DEV-005）— 仅 Android 桥可用；浏览器 mock 不支持
  // -------------------------------------------------------------------------

  /** 启动同步引擎，返回本机端点地址 JSON（null = 不可用/启动失败）。 */
  startSyncEngine(): string | null {
    return bridge()?.startSyncEngine() ?? null;
  },

  /** 获取本机端点地址 JSON。 */
  getSyncLocalAddr(): string | null {
    return bridge()?.getSyncLocalAddr() ?? null;
  },

  /** 与对端同步指定笔记（阻塞直至完成）。 */
  syncNote(peerAddrJson: string, noteId: string): P2pSyncReport | null {
    const b = bridge();
    if (!b) return null;
    try {
      return JSON.parse(b.syncNote(peerAddrJson, noteId)) as P2pSyncReport;
    } catch {
      return null;
    }
  },

  /** 启动接收循环（对端发起时自动合并+持久化）。 */
  startAcceptSync(noteId: string): boolean {
    return bridge()?.startAcceptSync(noteId) ?? false;
  },

  /** 关闭同步引擎。 */
  closeSyncEngine(): void {
    bridge()?.closeSyncEngine();
  },
};
