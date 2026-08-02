/**
 * Core event bus types.
 * Mirrors `crates/aurora-core/src/event_bus/event.rs` (`CoreEvent` enum)
 * as a TypeScript discriminated union keyed on `type`.
 */

import type { JsonValue } from './blocks';

/** Index type (mirrors `IndexType`). */
export type IndexType = 'full_text' | 'vector' | 'link';

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
  | { type: 'TaskCreated'; task_id: string; title: string }
  | { type: 'TaskUpdated'; task_id: string; status: string }
  | { type: 'AssetAdded'; asset_hash: string; mime_type: string }
  | { type: 'IndexRebuildRequest'; index_type: IndexType };

/** A handler for core events. */
export type CoreEventHandler = (event: CoreEvent) => void;
