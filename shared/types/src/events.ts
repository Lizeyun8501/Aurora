/**
 * Core event bus types.
 * Mirrors `crates/aurora-core/src/event_bus/event.rs` (`CoreEvent` enum)
 * as a TypeScript discriminated union keyed on `type`.
 */

import type { JsonValue } from './blocks';

/** Index type (mirrors `IndexType`). */
export type IndexType = 'full_text' | 'vector' | 'link';

/** 索引重建范围（mirrors `IndexRebuildScope`）。 */
export type IndexRebuildScope =
  | { kind: 'full' }
  | { kind: 'docs'; docs: string[] };

/** 任务依赖操作（mirrors `DependencyOp`）。 */
export type DependencyOp = 'add' | 'remove';

/** 复习评分（FSRS 四档，mirrors `Rating`）。 */
export type Rating = 'again' | 'hard' | 'good' | 'easy';

/** 冲突解决策略（mirrors `ConflictStrategy`）。 */
export type ConflictStrategy = 'keep_local' | 'keep_remote' | 'keep_both';

/** 事件通道（决定持久化与延迟策略；mirrors `EventChannel`）。 */
export type EventChannel = 'high' | 'medium' | 'low';

/** 事件生产者（mirrors `Actor`）。 */
export type Actor =
  | { kind: 'user' }
  | { kind: 'agent'; session_id: string }
  | { kind: 'system' }
  | { kind: 'sync'; device_id: string };

/**
 * 事件通用信封（mirrors `Envelope`）— 持久化层使用的完整包装。
 * 顺序保证作用域为 `aggregate_id`（同一聚合严格有序，跨聚合可并行）。
 */
export interface Envelope {
  /** 幂等键（UUIDv7）。 */
  event_id: string;
  /** 判别名，如 `"NoteCreated"`。 */
  event_type: string;
  /** 聚合根 ID；seq 排序作用域。 */
  aggregate_id: string;
  /** 通道。 */
  channel: EventChannel;
  /** 全局单调序号（作用域：aggregate_id）。 */
  seq: number;
  /** 生产者。 */
  actor: Actor;
  /** 毫秒时间戳。 */
  occurred_at: number;
  /** payload（`CoreEvent` 的 serde 序列化）。 */
  payload: JsonValue;
  /** payload 结构版本。 */
  schema_ver: number;
}

/**
 * 事件字典契约元数据（mirrors `CoreEvent::DICT`）— 41 类，V26 表 4-1 冻结。
 * 新增事件必须同步此表、`event.rs` 并走 ADR。
 */
export const EVENT_DICT: readonly {
  type: string;
  channel: EventChannel;
  persistent: boolean;
}[] = [
  { type: 'DocumentChanged', channel: 'medium', persistent: true },
  { type: 'BlockChanged', channel: 'medium', persistent: true },
  { type: 'NoteCreated', channel: 'medium', persistent: true },
  { type: 'NoteDeleted', channel: 'medium', persistent: true },
  { type: 'NoteRestored', channel: 'medium', persistent: true },
  { type: 'NoteMoved', channel: 'medium', persistent: true },
  { type: 'NoteTitleChanged', channel: 'medium', persistent: true },
  { type: 'BacklinksUpdated', channel: 'medium', persistent: true },
  { type: 'LinkCreated', channel: 'medium', persistent: true },
  { type: 'LinkRemoved', channel: 'medium', persistent: true },
  { type: 'TaskCreated', channel: 'medium', persistent: true },
  { type: 'TaskUpdated', channel: 'medium', persistent: true },
  { type: 'TaskCompleted', channel: 'medium', persistent: true },
  { type: 'TaskDependencyChanged', channel: 'medium', persistent: true },
  { type: 'TaskDue', channel: 'low', persistent: true },
  { type: 'TaskTimerStarted', channel: 'high', persistent: false },
  { type: 'TaskTimerStopped', channel: 'medium', persistent: true },
  { type: 'HabitChecked', channel: 'medium', persistent: true },
  { type: 'ReviewCardDue', channel: 'low', persistent: true },
  { type: 'ReviewCardRated', channel: 'medium', persistent: true },
  { type: 'SyncProgress', channel: 'high', persistent: false },
  { type: 'SyncCompleted', channel: 'low', persistent: true },
  { type: 'SyncFailed', channel: 'medium', persistent: true },
  { type: 'SyncDegraded', channel: 'medium', persistent: true },
  { type: 'SyncConflictDetected', channel: 'medium', persistent: true },
  { type: 'ConflictResolved', channel: 'medium', persistent: true },
  { type: 'DevicePaired', channel: 'medium', persistent: true },
  { type: 'DeviceRevoked', channel: 'medium', persistent: true },
  { type: 'AIGenerationComplete', channel: 'medium', persistent: true },
  { type: 'AILiquifyProposed', channel: 'high', persistent: false },
  { type: 'AIProposalCommitted', channel: 'medium', persistent: true },
  { type: 'AgentSessionStarted', channel: 'medium', persistent: true },
  { type: 'AgentToolInvoked', channel: 'medium', persistent: true },
  { type: 'AgentSessionExpired', channel: 'low', persistent: true },
  { type: 'AgentKilled', channel: 'high', persistent: true },
  { type: 'PermissionChanged', channel: 'medium', persistent: true },
  { type: 'PluginLoaded', channel: 'low', persistent: true },
  { type: 'PluginUnloaded', channel: 'low', persistent: true },
  { type: 'PluginPermissionDenied', channel: 'medium', persistent: true },
  { type: 'AssetAdded', channel: 'medium', persistent: true },
  { type: 'IndexRebuildRequest', channel: 'low', persistent: true },
] as const;

/** A single block change (mirrors `BlockChangeInfo`). */
export interface BlockChangeInfo {
  block_id: string;
  /** Operation type, e.g. `"insert"`, `"update"`, `"delete"`. */
  op_type: string;
}

/** Document change summary (mirrors `DocumentChangeSummary`). */
export interface DocumentChangeSummary {
  doc_id: string;
  changed_blocks: BlockChangeInfo[];
}

/** A permission entry (mirrors `PermissionEntry`). */
export interface PermissionEntry {
  role: string;
  actions: string[];
}

/** A permission set (mirrors `PermissionSet`). */
export interface PermissionSet {
  resource_id: string;
  owner: string;
  permissions: PermissionEntry[];
}

/**
 * Core event — discriminated union mirroring the Rust `CoreEvent` enum.
 * The `type` field is the discriminant (serde variant name).
 */
export type CoreEvent =
  | { type: 'DocumentChanged'; doc_id: string; change_summary: DocumentChangeSummary }
  | { type: 'SyncProgress'; target_id: string; progress: number }
  | { type: 'TaskDue'; task_id: string; due_time: number }
  | { type: 'AIGenerationComplete'; request_id: string; output: string }
  | { type: 'PermissionChanged'; resource_id: string; new_perms: PermissionSet }
  | { type: 'PluginLoaded'; plugin_id: string }
  | {
      type: 'BlockChanged';
      doc_id: string;
      block_id: string;
      block_type: string;
      content: JsonValue;
    }
  | { type: 'BacklinksUpdated'; doc_id: string }
  | { type: 'TaskCreated'; task_id: string; title: string; note_id: string | null; block_id: string | null }
  | { type: 'TaskUpdated'; task_id: string; status: string; changed_fields: string[] }
  | { type: 'TaskCompleted'; task_id: string; completed_at: number; actual_minutes: number | null }
  | { type: 'TaskDependencyChanged'; task_id: string; depends_on: string; op: DependencyOp }
  | { type: 'TaskTimerStarted'; task_id: string; started_at: number }
  | { type: 'TaskTimerStopped'; task_id: string; elapsed_minutes: number }
  | { type: 'HabitChecked'; habit_id: string; date: string; streak: number }
  | { type: 'ReviewCardDue'; card_id: string; note_id: string; scheduled_at: number }
  | { type: 'ReviewCardRated'; card_id: string; rating: Rating; next_due: number }
  | { type: 'SyncCompleted'; target_id: string; docs_synced: number; bytes: number }
  | { type: 'SyncFailed'; target_id: string; error_code: string; retryable: boolean }
  | { type: 'SyncDegraded'; from_target: string; to_target: string; reason: string }
  | { type: 'SyncConflictDetected'; doc_id: string; local_v: number; remote_v: number }
  | { type: 'ConflictResolved'; doc_id: string; strategy: ConflictStrategy; kept: string }
  | { type: 'DevicePaired'; device_id: string; pubkey: string; alias: string }
  | { type: 'DeviceRevoked'; device_id: string; revoked_by: string }
  | { type: 'AILiquifyProposed'; request_id: string; proposals: JsonValue[] }
  | { type: 'AIProposalCommitted'; request_id: string; accepted_ids: string[] }
  | { type: 'AgentSessionStarted'; session_id: string; agent_id: string; permissions: string[] }
  | { type: 'AgentToolInvoked'; session_id: string; tool: string; params: JsonValue; result: string }
  | { type: 'AgentSessionExpired'; session_id: string; expired_at: number }
  | { type: 'AgentKilled'; session_id: string; reason: string }
  | { type: 'PluginUnloaded'; plugin_id: string; reason: string }
  | { type: 'PluginPermissionDenied'; plugin_id: string; capability: string }
  | { type: 'AssetAdded'; asset_hash: string; mime_type: string; size: number }
  | { type: 'NoteCreated'; note_id: string; workspace_id: string; title: string }
  | { type: 'NoteDeleted'; note_id: string; origin_path: string }
  | { type: 'NoteRestored'; note_id: string; target_parent_id: string }
  | { type: 'NoteMoved'; note_id: string; from_parent: string; to_parent: string }
  | { type: 'NoteTitleChanged'; note_id: string; old_title: string; new_title: string }
  | { type: 'LinkCreated'; source_id: string; target_id: string; link_type: string }
  | { type: 'LinkRemoved'; source_id: string; target_id: string }
  | { type: 'IndexRebuildRequest'; index_type: IndexType; scope: IndexRebuildScope };

/** A handler for core events. */
export type CoreEventHandler = (event: CoreEvent) => void;
