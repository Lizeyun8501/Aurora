/**
 * Sync domain types.
 * Mirrors the concepts in `crates/aurora-sync/` (device, conflict, p2p, cloud).
 */

/** Device unique identifier. */
export type DeviceId = string;

/** Peer unique identifier (LAN/P2P). */
export type PeerId = string;

/** Sync target kind. */
export type SyncTargetKind =
  | 'cloud'
  | 'lan'
  | 'p2p'
  | 'webdav'
  | 'git'
  | 'calendar'
  | 'cloud_drive'
  | 'email'
  | 'webhook';

/** Sync target status. */
export type SyncStatus =
  | 'idle'
  | 'syncing'
  | 'paused'
  | 'error'
  | 'offline'
  | 'conflict';

/** Device status (mirrors `DeviceStatus`). */
export type DeviceStatus = 'online' | 'offline' | 'syncing';

/** Sync progress (mirrors sync progress reporting). */
export interface SyncProgress {
  target_id: string;
  total: number;
  done: number;
  failed: number;
  /** 0.0 – 1.0. */
  progress: number;
  current_item: string | null;
  started_at: string | null;
  error: string | null;
}

/** A configured sync target. */
export interface SyncTarget {
  id: string;
  name: string;
  kind: SyncTargetKind;
  status: SyncStatus;
  /** Opaque, target-specific configuration (credentials stored securely). */
  config: Record<string, unknown>;
  last_synced_at: string | null;
  enabled: boolean;
}

/** A known device. */
export interface Device {
  id: DeviceId;
  name: string;
  platform: string;
  status: DeviceStatus;
  last_seen: string | null;
}

/** Conflict resolution strategy. */
export type ConflictResolutionStrategy =
  | 'last_write_wins'
  | 'first_write_wins'
  | 'manual'
  | 'merge'
  | 'ours'
  | 'theirs';

/** A sync conflict and its resolution. */
export interface ConflictResolution {
  conflict_id: string;
  resource_id: string;
  /** The conflicting revisions. */
  ours: unknown;
  theirs: unknown;
  strategy: ConflictResolutionStrategy;
  /** The resolved value (after applying `strategy` or manual pick). */
  resolved: unknown;
  resolved_at: string | null;
}
