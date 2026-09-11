/**
 * CoreAPI 契约 — V26 DK-00 冻结（§5.22）
 *
 * 双端共用：桌面经 Tauri invoke，移动经 JNI bridge。
 * 以 mobile-ffi 现有 15 个 JNI 函数为第一版基线（nativeNew/nativeDestroy 为
 * 生命周期桩，不计入业务 API — 13 业务函数 + applyNoteChange/pollEvents 两
 * 个 V26 新增 = CoreAPI 15 方法）。
 *
 * ## 三条纪律（V26 §5.22）
 * 1. 前端不持有业务状态：所有 VM 由 Rust 产出，前端只渲染与交互
 * 2. AI 两段式：`aiLiquify` 出提案，`aiCommit` 才落库（禁止静默写入）
 * 3. 移动端用 `pollEvents`（16ms 轮询）而非 FFI 回调，避免跨线程安全问题
 *
 * 契约变更必须走 ADR（docs/adr/ADR-002-contract-governance.md）。
 */

import type { Envelope } from '../../types/src/events';

// ===== 视图模型 —— 由 Rust 产出，前端只渲染 =====

/** 笔记视图模型。 */
export interface NoteVM {
  id: string;
  title: string;
  blocks: BlockVM[];
  tags: string[];
  updatedAt: string;
  /** 无障碍语义（铁律 8：由内核携带，前端不得自行补全）。 */
  a11y: { label: string; hint?: string };
}

/** 块视图模型。 */
export interface BlockVM {
  id: string;
  type: string;
  content: unknown;
  children?: BlockVM[];
  a11y: { label: string; hint?: string };
}

/** 任务视图模型。 */
export interface TaskVM {
  id: string;
  title: string;
  status: string;
  /** 子任务进度推导值（TaskEngine::derive_progress，禁止手改）。 */
  progress: number;
  dependsOn: string[];
  nextDue?: string;
  a11y: { label: string; hint?: string };
}

/** 反向链接视图模型。 */
export interface BacklinkVM {
  source_id: string;
  source_title: string;
  context: string;
}

/** 检索结果视图模型。 */
export interface SearchResultVM {
  note_id: string;
  title: string;
  score: number;
  snippet: string;
}

/** AI 液化提案视图模型（两段式第一段产出）。 */
export interface ProposalVM {
  id: string;
  kind: 'liquify' | 'summary' | 'rewrite';
  /** 建议产出的块序列（草稿，未落库）。 */
  draft_blocks: unknown[];
  rationale: string;
}

/** 写入回执 — WritePath 唯一入口的返回凭据（DK-01W 对齐）。 */
export interface WriteReceipt {
  /** 本次写入产生的事件序号（投影水位线推进依据）。 */
  seq: number;
  /** 受影响聚合 ID。 */
  aggregate_id: string;
  /** 服务端毫秒时间戳。 */
  committed_at: number;
}

// ===== 输入类型 =====

export interface NoteFilter {
  workspace?: string;
  tag?: string;
  limit?: number;
}

/** 笔记变更操作（applyNoteChange 唯一写入入口的操作代数）。 */
export type NoteOp =
  | { kind: 'create'; title: string; parent: string | null }
  | { kind: 'update_blocks'; ops: unknown[] }
  | { kind: 'rename'; new_title: string };

export interface TaskInput {
  title: string;
  note_id?: string;
  block_id?: string;
}

export interface TaskPatch {
  title?: string;
  status?: string;
}

export interface SearchOptions {
  limit?: number;
  semantic?: boolean;
}

export interface TextSelection {
  note_id: string;
  start: number;
  end: number;
}

export type AiAction = 'liquify' | 'summary' | 'rewrite';

// ===== 契约本体 =====

export interface CoreAPI {
  // ---- 笔记 ----
  listNotes(ws: string, filter?: NoteFilter): Promise<NoteVM[]>;
  getNote(id: string): Promise<NoteVM>;
  /** 唯一写入入口（DK-01W：禁止直调 kv_store.set）。 */
  applyNoteChange(id: string, op: NoteOp): Promise<WriteReceipt>;
  /** 移动笔记（含环检测 — F01 拒绝）。 */
  moveNote(id: string, toParent: string | null): Promise<void>;
  /** 软删除进回收站。 */
  deleteNote(id: string): Promise<void>;

  // ---- 任务 ----
  listTodayTasks(): Promise<TaskVM[]>;
  createTask(input: TaskInput): Promise<TaskVM>;
  updateTask(id: string, patch: TaskPatch): Promise<TaskVM>;
  /** 添加依赖（TaskEngine 端口做环检测 — F02 拒绝）。 */
  addDependency(task: string, dependsOn: string): Promise<void>;

  // ---- 检索 ----
  search(q: string, opt?: SearchOptions): Promise<SearchResultVM[]>;
  getBacklinks(noteId: string): Promise<BacklinkVM[]>;

  // ---- AI（两段式，禁止静默写入）----
  aiLiquify(sel: TextSelection, action: AiAction): Promise<ProposalVM[]>;
  aiCommit(requestId: string, accepted: string[]): Promise<WriteReceipt>;

  // ---- 事件（移动端轮询，避免 FFI 回调跨线程）----
  pollEvents(since: number): Promise<Envelope[]>;
}

/** JNI 基线映射（第一版）：CoreAPI 方法 ↔ mobile-ffi native 函数。 */
export const JNI_BASELINE: readonly { api: keyof CoreAPI | null; jni: string | null }[] = [
  { api: 'listNotes', jni: 'nativeListNotesCount + nativeGetNote' },
  { api: 'getNote', jni: 'nativeGetNote' },
  { api: 'applyNoteChange', jni: 'nativeSaveNoteContent (V26 拓展为 WritePath)' },
  { api: 'deleteNote', jni: 'nativeDeleteNote' },
  { api: 'search', jni: 'nativeSearchCount + nativeGetSearchResult' },
  { api: 'listTodayTasks', jni: 'nativeGetNoteSnapshot (派生)' },
  { api: 'pollEvents', jni: 'nativeListSnapshots (V26 拓展为事件流)' },
  { api: null, jni: 'nativeNew / nativeDestroy (生命周期桩)' },
  { api: null, jni: 'nativeIsFallback (降级探针)' },
  { api: null, jni: 'nativeSaveNoteSnapshot / nativeGetSnapshotContent (快照存取)' },
] as const;
